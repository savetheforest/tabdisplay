package com.tabdisplay

import android.content.Context
import android.media.MediaCodec
import android.media.MediaFormat
import android.os.Handler
import android.os.HandlerThread
import android.view.Surface
import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.EOFException
import java.net.ConnectException
import java.net.InetSocketAddress
import java.net.NoRouteToHostException
import java.net.Socket
import java.net.SocketException
import java.net.SocketTimeoutException
import java.nio.ByteBuffer
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

// Wire protocol v2: see PROTOCOL.md at the repo root.
private const val PORT = 7070
private const val HELLO = 1
private const val VIDEO = 2
private const val INPUT = 3
private const val CONFIG = 4
private const val PAIR_REQUIRED = 5
private const val PAIR = 6
private const val PAIRED = 7
private const val ERROR = 8
private const val RESIZE = 9
private const val PING = 10
private const val PONG = 11
private const val STATS = 12
private const val SCROLL = 13
private const val PROFILE = 14
private const val MAX_MSG = 16 shl 20

/** What a session reports back to the UI. Called from the stream thread. */
interface StreamEvents {
    fun onVideoSize(width: Int, height: Int)
    /** The PC shows a 6-digit code; answer with [Stream.pair]. `wrong` = the last code didn't match. */
    fun onPairRequired(pcName: String, wrong: Boolean)
    fun onPaired(pcId: String, token: String)
    fun onClose(reason: String)
    /** Once a second: frames this tablet showed, round trip and data rate (as measured by the PC). */
    fun onStats(shownFps: Int, rttMs: Int, mbps: Double) {}
    /** The quality preset the PC uses now: "performance", "balanced", "quality" or "custom". */
    fun onProfile(profile: String) {}
}

/** One session with the PC: reads video into a MediaCodec rendering to [surface], sends touches back. */
class Stream(
    private val context: Context,
    private val host: String,
    surface: Surface,
    private val hello: JSONObject,
    private val events: StreamEvents,
) {
    private val socket = Socket()
    private val sender = Executors.newSingleThreadExecutor()
    private val closed = AtomicBoolean(false)
    private val freeInputs = LinkedBlockingQueue<Int>()
    // The decoder's callbacks need their own Looper; without one Android delivers them on the main
    // thread, where they'd compete with Compose recomposition and touch dispatch for time and cap fps.
    private val decoderThread = HandlerThread("decoder").apply { start() }
    private val decoderHandler = Handler(decoderThread.looper)
    private lateinit var out: DataOutputStream
    @Volatile private var codec: MediaCodec? = null
    /** Guards [codec] and [surface]: the reader thread decodes while the UI thread attaches/detaches the surface. */
    private val lock = Any()
    private var surface: Surface? = surface
    /** Last CONFIG size; lets the decoder restart when a new surface shows up. */
    @Volatile private var videoSize: Pair<Int, Int>? = null
    /** Frames shown since the last PING (the PC pings once a second, so this is fps). */
    private val rendered = java.util.concurrent.atomic.AtomicInteger()
    @Volatile private var connected = false
    /** Set when the PC explains why it ends the session (ERROR message). */
    @Volatile private var pcReason: String? = null

    fun start() = thread(name = "stream") {
        val reason = try {
            run()
            context.getString(R.string.pc_closed)
        } catch (e: Exception) {
            pcReason ?: friendly(e)
        }
        if (close()) events.onClose(reason)
    }

    private fun friendly(e: Exception): String = when {
        !connected && (e is SocketTimeoutException || e is ConnectException || e is NoRouteToHostException) ->
            context.getString(R.string.pc_no_response)
        connected && (e is EOFException || e is SocketException) -> context.getString(R.string.connection_dropped)
        else -> context.getString(R.string.error_generic, e.message ?: e.javaClass.simpleName)
    }

    /** Returns true only for the call that actually closed the session. */
    fun close(): Boolean {
        if (!closed.compareAndSet(false, true)) return false
        runCatching { socket.close() }
        sender.shutdownNow()
        synchronized(lock) { releaseCodec() }
        decoderThread.quitSafely()
        return true
    }

    private fun releaseCodec() {
        codec?.let { runCatching { it.stop(); it.release() } }
        codec = null
    }

    /** The screen went away (lock, app switch): stop decoding but keep the connection, so the PC keeps the monitor. */
    fun detach() = synchronized(lock) {
        surface = null
        releaseCodec()
    }

    /** The screen is back: the PC re-sends CONFIG and a keyframe (it rebuilds on RESIZE), which restarts the decoder. */
    fun attach(newSurface: Surface, decodableW: Int, decodableH: Int) {
        synchronized(lock) { surface = newSurface }
        if (videoSize != null) resize(decodableW, decodableH)
    }

    /** One INPUT frame: every touch/pen contact of a MotionEvent, already encoded (see [Input]). */
    fun input(frame: ByteArray) = send(INPUT) { write(frame) }

    /** One SCROLL frame (mouse wheel / trackpad), already encoded (see [Input.scroll]). */
    fun scroll(frame: ByteArray) = send(SCROLL) { write(frame) }

    /** Asks the PC to switch to a quality preset ("performance", "balanced" or "quality"). */
    fun setProfile(profile: String) = sendJson(PROFILE, JSONObject().put("profile", profile))

    /** The tablet rotated: ask for video (and a virtual monitor) of the new decodable size. */
    fun resize(width: Int, height: Int) = sendJson(RESIZE, JSONObject().put("decodable", org.json.JSONArray(listOf(width, height))))

    fun pair(code: String) = sendJson(PAIR, JSONObject().put("code", code))

    private fun run() {
        socket.tcpNoDelay = true
        socket.connect(InetSocketAddress(host, PORT), 3000)
        connected = true
        out = DataOutputStream(BufferedOutputStream(socket.getOutputStream()))
        sendJson(HELLO, hello)

        val input = DataInputStream(BufferedInputStream(socket.getInputStream(), 1 shl 16))
        while (!closed.get()) {
            val type = input.readUnsignedByte()
            val len = input.readInt()
            require(len in 0..MAX_MSG) { context.getString(R.string.invalid_message) }
            val payload = ByteArray(len)
            input.readFully(payload)
            when (type) {
                VIDEO -> decode(payload)
                CONFIG -> json(payload).let { startDecoder(it.getInt("width"), it.getInt("height")) }
                PAIR_REQUIRED -> json(payload).let { events.onPairRequired(it.optString("pc_name", "PC"), it.optBoolean("wrong")) }
                PAIRED -> json(payload).let { events.onPaired(it.getString("pc_id"), it.getString("token")) }
                PROFILE -> json(payload).let { events.onProfile(it.optString("profile")) }
                ERROR -> pcReason = json(payload).optString("message", context.getString(R.string.pc_refused))
                PING -> {
                    val ping = json(payload)
                    val shown = rendered.getAndSet(0)
                    sendJson(PONG, JSONObject().put("t", ping.getLong("t"))) // echo: the PC measures the round trip
                    sendJson(STATS, JSONObject().put("fps", shown))
                    events.onStats(shown, ping.optInt("rtt_ms"), ping.optDouble("mbps"))
                }
            }
        }
    }

    private fun json(payload: ByteArray) = JSONObject(String(payload, Charsets.UTF_8))

    private fun sendJson(type: Int, obj: JSONObject) = send(type) { write(obj.toString().toByteArray(Charsets.UTF_8)) }

    private fun send(type: Int, body: DataOutputStream.() -> Unit) {
        if (closed.get()) return
        val bytes = java.io.ByteArrayOutputStream().also { DataOutputStream(it).body() }
        runCatching {
            sender.execute {
                runCatching {
                    out.writeByte(type)
                    out.writeInt(bytes.size())
                    bytes.writeTo(out)
                    out.flush()
                }
            }
        }
    }

    private fun startDecoder(width: Int, height: Int) {
        videoSize = width to height
        events.onVideoSize(width, height)
        synchronized(lock) {
            releaseCodec()
            freeInputs.clear()
            val target = surface ?: return // no screen right now; attach() asks the PC for a new CONFIG
            codec = newDecoder(width, height, target)
        }
    }

    private fun newDecoder(width: Int, height: Int, target: Surface): MediaCodec {
        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
            setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
            setInteger(MediaFormat.KEY_OPERATING_RATE, 60) // hints the SoC to keep the video core clocked up
            setInteger(MediaFormat.KEY_PRIORITY, 0) // realtime
        }
        return MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC).apply {
            // On `decoderHandler`'s own thread, off the main looper: Compose recomposition and touch
            // dispatch on the main thread would otherwise delay these and cap how fast frames render.
            setCallback(object : MediaCodec.Callback() {
                override fun onInputBufferAvailable(c: MediaCodec, index: Int) {
                    freeInputs.put(index)
                }

                override fun onOutputBufferAvailable(c: MediaCodec, index: Int, info: MediaCodec.BufferInfo) {
                    runCatching { c.releaseOutputBuffer(index, true) } // render immediately, no pacing
                    rendered.incrementAndGet()
                }

                override fun onError(c: MediaCodec, e: MediaCodec.CodecException) {
                    runCatching { socket.close() } // unblocks the reader, which reports the error
                }

                override fun onOutputFormatChanged(c: MediaCodec, format: MediaFormat) = Unit
            }, decoderHandler)
            configure(format, target, null, 0)
            start()
        }
    }

    private fun decode(frame: ByteArray) {
        val c = codec ?: return
        // ponytail: drops the frame if the decoder is stalled; can smear until the next keyframe.
        val index = freeInputs.poll(500, TimeUnit.MILLISECONDS) ?: return
        synchronized(lock) {
            if (codec !== c) return // released meanwhile (surface detached or new CONFIG)
            c.getInputBuffer(index)!!.apply {
                clear()
                put(frame)
            }
            c.queueInputBuffer(index, 0, frame.size, System.nanoTime() / 1000, 0)
        }
    }
}
