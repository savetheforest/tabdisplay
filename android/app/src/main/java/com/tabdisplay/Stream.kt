package com.tabdisplay

import android.annotation.SuppressLint
import android.content.Context
import android.media.MediaCodec
import android.media.MediaFormat
import android.os.Handler
import android.os.HandlerThread
import android.view.Surface
import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.DataOutputStream
import java.io.EOFException
import java.net.ConnectException
import java.net.InetSocketAddress
import java.net.NoRouteToHostException
import java.net.Socket
import java.net.SocketException
import java.net.SocketTimeoutException
import java.nio.ByteBuffer
import java.security.MessageDigest
import java.security.cert.X509Certificate
import javax.net.ssl.SSLContext
import javax.net.ssl.SSLSocket
import javax.net.ssl.X509TrustManager
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.RejectedExecutionException
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong
import kotlin.concurrent.thread

private const val PORT = 7070
private const val HELLO = Protocol.HELLO
private const val VIDEO = Protocol.VIDEO
private const val INPUT = Protocol.INPUT
private const val CONFIG = Protocol.CONFIG
private const val PAIR_REQUIRED = Protocol.PAIR_REQUIRED
private const val PAIR = Protocol.PAIR
private const val PAIRED = Protocol.PAIRED
private const val ERROR = Protocol.ERROR
private const val RESIZE = Protocol.RESIZE
private const val PING = Protocol.PING
private const val PONG = Protocol.PONG
private const val STATS = Protocol.STATS
private const val SCROLL = Protocol.SCROLL
private const val PROFILE = Protocol.PROFILE
private const val AUDIO = Protocol.AUDIO
private const val KEYFRAME = Protocol.KEYFRAME
private const val PAUSE = Protocol.PAUSE
private const val RESUME = Protocol.RESUME
private const val TEXT = Protocol.TEXT
private const val KEY = Protocol.KEY

private data class CodecInput(val codec: MediaCodec, val index: Int)
private data class QueuedVideo(val configId: Long, val bytes: ByteArray)

/** What a session reports back to the UI. Called from the stream thread. */
interface StreamEvents {
    fun onVideoSize(width: Int, height: Int)
    /** The PC shows a 6-digit code; answer with [Stream.pair]. `wrong` = the last code didn't match. */
    fun onPairRequired(pcName: String, wrong: Boolean)
    fun onPaired(pcId: String, token: String)
    fun onClose(reason: String)
    /** Once a second: render evidence, round trip and data rate (as measured by the PC). */
    fun onStats(shownFps: Int, rttMs: Int, mbps: Double) {}
    /** The quality preset the PC uses now: "performance", "balanced", "quality", "auto" or "custom". */
    fun onProfile(profile: String) {}
    /**
     * The PC's TLS certificate (SHA-256 of its DER, hex). Return false to refuse it: the app pins the first
     * certificate it sees for each PC (trust on first use), so a different one means another machine answered.
     */
    fun onServerCertificate(fingerprint: String): Boolean = true
    /** A manually requested text payload from the PC; the UI decides whether to copy it. */
    fun onTextReceived(sender: String, text: String) {}
}

/** One session with the PC: reads video into a MediaCodec rendering to [surface], sends touches back. */
class Stream(
    private val context: Context,
    private val host: String,
    surface: Surface,
    private val hello: JSONObject,
    private val decoderNames: List<String>,
    private val events: StreamEvents,
) {
    private val socket = Socket()
    // A bounded queue prevents touch/scroll storms from retaining an unbounded number of frames.
    // Rejection is a session failure, not a silently dropped control/input write.
    private val sender = ThreadPoolExecutor(
        1, 1, 0L, TimeUnit.MILLISECONDS, ArrayBlockingQueue(64), ThreadPoolExecutor.AbortPolicy(),
    )
    private val closed = AtomicBoolean(false)
    private val sendFailed = AtomicBoolean(false)
    private val freeInputs = ArrayBlockingQueue<CodecInput>(32)
    private val queuedInputIndices = HashSet<Int>()
    private val pendingVideo = ArrayBlockingQueue<QueuedVideo>(4)
    private val decodeDrainScheduled = AtomicBoolean(false)
    // The decoder's callbacks need their own Looper; without one Android delivers them on the main
    // thread, where they'd compete with Compose recomposition and touch dispatch for time and cap fps.
    private val decoderThread = HandlerThread("decoder").apply { start() }
    private val decoderHandler = Handler(decoderThread.looper)
    private val inputThread = HandlerThread("decoder-input").apply { start() }
    private val inputHandler = Handler(inputThread.looper)
    private lateinit var out: DataOutputStream
    @Volatile private var codec: MediaCodec? = null
    /** Guards [codec] and [surface]: the reader thread decodes while the UI thread attaches/detaches the surface. */
    private val lock = Any()
    private var surface: Surface? = surface
    /** Last CONFIG size; lets the decoder restart when a new surface shows up. */
    @Volatile private var videoSize: Pair<Int, Int>? = null
    /** Frames shown since the last PING (the PC pings once a second, so this is fps). */
    private val rendered = java.util.concurrent.atomic.AtomicInteger()
    private val decoded = java.util.concurrent.atomic.AtomicInteger()
    private val droppedDecode = java.util.concurrent.atomic.AtomicInteger()
    private val renderFailures = java.util.concurrent.atomic.AtomicInteger()
    @Volatile private var configId = 0L
    @Volatile private var needsKeyframe = true
    private val lastKeyframeRequestNs = AtomicLong(0L)
    private var audio: Audio? = null
    /** Local mute of the PC's audio; the PC's own volume is not touched. */
    @Volatile var muted = false
        set(value) {
            field = value
            audio?.muted = value
        }
    @Volatile private var connected = false
    @Volatile private var secured = false
    /** Set when the PC explains why it ends the session (ERROR message). */
    @Volatile private var pcReason: String? = null

    fun start() = thread(name = "stream") {
        val reason = try {
            run()
            Crumbs.add("stream", "closed by the PC")
            context.getString(R.string.pc_closed)
        } catch (e: Exception) {
            Crumbs.add("stream", "error: ${e.javaClass.simpleName}")
            if (Crumbs.unexpected(e)) io.sentry.Sentry.captureException(e)
            pcReason ?: friendly(e)
        }
        if (close()) events.onClose(reason)
    }

    private fun friendly(e: Exception): String = when {
        e is SecurityException -> e.message ?: context.getString(R.string.identity_changed)
        connected && !secured -> context.getString(R.string.tls_failed)
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
        pendingVideo.clear()
        synchronized(lock) { releaseCodec() }
        audio?.release()
        audio = null
        decoderThread.quitSafely()
        inputThread.quitSafely()
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
        sendJson(PAUSE, JSONObject())
    }

    /** The screen is back: the PC re-sends CONFIG and a keyframe (it rebuilds on RESIZE), which restarts the decoder. */
    fun attach(newSurface: Surface, decodableW: Int, decodableH: Int) {
        synchronized(lock) { surface = newSurface }
        sendJson(RESUME, JSONObject())
        if (videoSize != null && videoSize != (decodableW to decodableH)) resize(decodableW, decodableH)
    }

    /** One INPUT frame: every touch/pen contact of a MotionEvent, already encoded (see [Input]). */
    fun input(frame: ByteArray) = send(INPUT) { write(frame) }

    /** One SCROLL frame (mouse wheel / trackpad), already encoded (see [Input.scroll]). */
    fun scroll(frame: ByteArray) = send(SCROLL) { write(frame) }

    /** Asks the PC to switch to a quality preset ("performance", "balanced", "quality" or "auto"). */
    fun setProfile(profile: String) = sendJson(PROFILE, JSONObject().put("profile", profile))

    /** The tablet rotated: ask for video (and a virtual monitor) of the new decodable size. */
    fun resize(width: Int, height: Int) {
        require(Protocol.validDimensions(width, height)) { "invalid video dimensions" }
        sendJson(RESIZE, JSONObject().put("decodable", org.json.JSONArray(listOf(width, height))))
    }

    fun pair(code: String) = sendJson(PAIR, JSONObject().put("code", code))

    /** Sends the current clipboard only after an explicit user action; never auto-pastes. */
    fun sendText(text: String) {
        require(Protocol.validText(text)) { "text too large" }
        sendJson(TEXT, JSONObject().put("version", 1).put("source", "tablet").put("text", text))
    }

    /** Sends committed IME text to the currently focused PC window; it is never pasted or logged. */
    fun sendKeyText(text: String) {
        require(Protocol.validText(text)) { "text too large" }
        sendJson(KEY, JSONObject().put("version", 1).put("source", "tablet").put("action", "text").put("text", text))
    }

    /** Sends one allow-listed logical key with optional Ctrl/Shift/Alt/Meta modifiers. */
    fun sendKey(key: String, modifiers: Int = 0) {
        require(modifiers and (Protocol.MOD_CTRL or Protocol.MOD_SHIFT or Protocol.MOD_ALT or Protocol.MOD_META).inv() == 0) { "invalid modifiers" }
        sendJson(KEY, JSONObject().put("version", 1).put("source", "tablet").put("action", "key").put("key", key).put("modifiers", modifiers).put("down", true))
        sendJson(KEY, JSONObject().put("version", 1).put("source", "tablet").put("action", "key").put("key", key).put("modifiers", modifiers).put("down", false))
    }

    private fun run() {
        socket.tcpNoDelay = true
        socket.connect(InetSocketAddress(host, PORT), 3000)
        connected = true
        Crumbs.add("stream", "tcp connected")
        val tls = secure()
        out = DataOutputStream(BufferedOutputStream(tls.getOutputStream()))
        sendJson(HELLO, hello)

        val input = Protocol.Reader(BufferedInputStream(tls.getInputStream(), 1 shl 16))
        while (!closed.get()) {
            val frame = input.read()
            val type = frame.type
            val payload = frame.payload
            when (type) {
                VIDEO -> decode(payload)
                CONFIG -> json(payload).let {
                    val width = it.getInt("width")
                    val height = it.getInt("height")
                    val fps = it.optInt("fps", 60)
                    val id = it.optLong("config_id", 0L)
                    require(Protocol.validDimensions(width, height)) { context.getString(R.string.invalid_message) }
                    require(fps in 1..Protocol.MAX_FPS) { context.getString(R.string.invalid_message) }
                    startDecoder(width, height, fps, id)
                }
                PAIR_REQUIRED -> {
                    Crumbs.add("stream", "pairing required")
                    json(payload).let { events.onPairRequired(it.optString("pc_name", "PC"), it.optBoolean("wrong")) }
                }
                PAIRED -> {
                    Crumbs.add("stream", "paired") // never the token
                    json(payload).let { events.onPaired(it.getString("pc_id"), it.getString("token")) }
                }
                AUDIO -> playAudio(payload)
                KEYFRAME -> throw IllegalArgumentException(context.getString(R.string.invalid_message))
                PROFILE -> json(payload).let { events.onProfile(it.optString("profile")) }
                ERROR -> pcReason = json(payload).optString("message", context.getString(R.string.pc_refused))
                TEXT -> {
                    val message = json(payload)
                    require(message.optInt("version", 0) == 1 && message.optString("source") == "pc") { context.getString(R.string.invalid_message) }
                    val text = message.optString("text", "")
                    require(Protocol.validText(text)) { context.getString(R.string.invalid_message) }
                    events.onTextReceived(message.optString("sender", "PC"), text)
                }
                PING -> {
                    val ping = json(payload)
                    val shown = rendered.getAndSet(0)
                    val decodedNow = decoded.getAndSet(0)
                    val droppedNow = droppedDecode.getAndSet(0)
                    val failuresNow = renderFailures.getAndSet(0)
                    sendJson(PONG, JSONObject().put("t", ping.getLong("t"))) // echo: the PC measures the round trip
                    sendJson(STATS, JSONObject()
                        .put("fps", shown)
                        .put("decode_fps", decodedNow)
                        .put("render_fps", shown)
                        .put("decode_drops", droppedNow)
                        .put("render_failures", failuresNow)
                        .put("config_id", configId))
                    events.onStats(shown, ping.optInt("rtt_ms"), ping.optDouble("mbps"))
                }
                else -> throw IllegalArgumentException(context.getString(R.string.invalid_message))
            }
        }
    }

    private fun playAudio(packet: ByteArray) {
        if (audio == null) audio = runCatching { Audio() }.getOrNull()?.also { it.muted = muted } ?: return
        runCatching { audio!!.play(AudioPacket.decode(packet)) }
    }

    /** TLS 1.3 over the connected socket, before anything (the pairing token!) is sent. */
    @SuppressLint("CustomX509TrustManager", "TrustAllX509TrustManager")
    private fun secure(): SSLSocket {
        // Any certificate is accepted by the handshake itself; identity is decided by the pin below.
        val anyCert = object : X509TrustManager {
            override fun checkClientTrusted(chain: Array<X509Certificate>, authType: String) = Unit
            override fun checkServerTrusted(chain: Array<X509Certificate>, authType: String) = Unit
            override fun getAcceptedIssuers(): Array<X509Certificate> = emptyArray()
        }
        val ssl = SSLContext.getInstance("TLSv1.3").apply { init(null, arrayOf(anyCert), null) }
            .socketFactory.createSocket(socket, host, PORT, true) as SSLSocket
        ssl.soTimeout = 5000 // a PC that doesn't speak TLS (older version) just stays silent
        ssl.startHandshake()
        ssl.soTimeout = 0
        val cert = ssl.session.peerCertificates[0].encoded
        val fingerprint = MessageDigest.getInstance("SHA-256").digest(cert).joinToString("") { "%02x".format(it) }
        if (!events.onServerCertificate(fingerprint)) throw SecurityException(context.getString(R.string.identity_changed))
        secured = true
        Crumbs.add("stream", "tls up")
        return ssl
    }

    private fun json(payload: ByteArray) = JSONObject(String(payload, Charsets.UTF_8))

    private fun sendJson(type: Int, obj: JSONObject) = send(type) { write(obj.toString().toByteArray(Charsets.UTF_8)) }

    private fun send(type: Int, body: DataOutputStream.() -> Unit) {
        if (closed.get()) return
        val bytes = java.io.ByteArrayOutputStream().also { DataOutputStream(it).body() }.toByteArray()
        val frame = Protocol.frame(type, bytes)
        try {
            sender.execute {
                try {
                    out.write(frame)
                    out.flush()
                } catch (_: Exception) {
                    failSend()
                }
            }
        } catch (_: RejectedExecutionException) {
            failSend()
        }
    }

    private fun failSend() {
        if (sendFailed.compareAndSet(false, true)) {
            pcReason = context.getString(R.string.connection_dropped)
            runCatching { socket.close() }
        }
    }

    private fun startDecoder(width: Int, height: Int, fps: Int, id: Long) {
        if (closed.get()) return
        videoSize = width to height
        configId = id
        needsKeyframe = true
        events.onVideoSize(width, height)
        synchronized(lock) {
            releaseCodec()
            freeInputs.clear()
            queuedInputIndices.clear()
            pendingVideo.clear()
            val target = surface ?: return // no screen right now; attach() asks the PC for a new CONFIG
            codec = newDecoder(width, height, fps, target)
        }
    }

    private fun newDecoder(width: Int, height: Int, fps: Int, target: Surface): MediaCodec {
        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
            runCatching { setInteger(MediaFormat.KEY_LOW_LATENCY, 1) }
            runCatching { setInteger(MediaFormat.KEY_OPERATING_RATE, fps) }
            runCatching { setInteger(MediaFormat.KEY_PRIORITY, 0) }
        }
        var last: Exception? = null
        for (name in decoderNames.distinct()) {
            var candidate: MediaCodec? = null
            try {
                val created = MediaCodec.createByCodecName(name)
                candidate = created
                return created.apply {
            // On `decoderHandler`'s own thread, off the main looper: Compose recomposition and touch
            // dispatch on the main thread would otherwise delay these and cap how fast frames render.
            setCallback(object : MediaCodec.Callback() {
                override fun onInputBufferAvailable(c: MediaCodec, index: Int) {
                    synchronized(lock) {
                        if (codec === c && !closed.get() && queuedInputIndices.add(index)) {
                            freeInputs.offer(CodecInput(c, index))
                        }
                    }
                }

                override fun onOutputBufferAvailable(c: MediaCodec, index: Int, info: MediaCodec.BufferInfo) {
                    val current = synchronized(lock) { codec === c && !closed.get() }
                    if (!current) return
                    if (runCatching { c.releaseOutputBuffer(index, true) }.isSuccess) {
                        rendered.incrementAndGet()
                    } else {
                        renderFailures.incrementAndGet()
                    }
                }

                override fun onError(c: MediaCodec, e: MediaCodec.CodecException) {
                    runCatching { socket.close() } // unblocks the reader, which reports the error
                }

                override fun onOutputFormatChanged(c: MediaCodec, format: MediaFormat) = Unit
            }, decoderHandler)
            configure(format, target, null, 0)
            start()
                }
            } catch (error: Exception) {
                last = error
                runCatching { candidate?.stop() }
                runCatching { candidate?.release() }
            }
        }
        throw last ?: IllegalStateException("no AVC decoder selected")
    }

    private fun decode(frame: ByteArray) {
        if (closed.get()) return
        if (!pendingVideo.offer(QueuedVideo(configId, frame))) {
            droppedDecode.incrementAndGet()
            requestKeyframe()
            return
        }
        if (decodeDrainScheduled.compareAndSet(false, true)) inputHandler.post(::drainDecode)
    }

    /** Runs away from the socket reader, so a slow codec cannot delay PING/PONG or control handling. */
    private fun drainDecode() {
        try {
            while (!closed.get()) {
                val queued = pendingVideo.poll(500, TimeUnit.MILLISECONDS) ?: break
                decodeOne(queued)
            }
        } finally {
            decodeDrainScheduled.set(false)
            if (!closed.get() && pendingVideo.isNotEmpty() && decodeDrainScheduled.compareAndSet(false, true)) {
                inputHandler.post(::drainDecode)
            }
        }
    }

    private fun decodeOne(queued: QueuedVideo) {
        val c = codec ?: return
        if (queued.configId != configId) return
        val frame = queued.bytes
        if (needsKeyframe && !containsH264Idr(frame)) {
            requestKeyframe()
            return
        }
        // ponytail: drops the frame if the decoder is stalled; can smear until the next keyframe.
        val input = freeInputs.poll(500, TimeUnit.MILLISECONDS) ?: run {
            droppedDecode.incrementAndGet()
            return
        }
        synchronized(lock) {
            if (codec !== c || input.codec !== c || closed.get()) return // released meanwhile (surface detached or new CONFIG)
            queuedInputIndices.remove(input.index)
            val buffer = c.getInputBuffer(input.index) ?: run {
                droppedDecode.incrementAndGet()
                requestKeyframe()
                return
            }
            if (frame.size > buffer.capacity()) {
                droppedDecode.incrementAndGet()
                requestKeyframe()
                return
            }
            runCatching {
                buffer.apply {
                clear()
                put(frame)
            }
                c.queueInputBuffer(input.index, 0, frame.size, System.nanoTime() / 1000, 0)
                decoded.incrementAndGet()
                needsKeyframe = false
            }.onFailure {
                droppedDecode.incrementAndGet()
                requestKeyframe()
            }
        }
    }

    private fun requestKeyframe() {
        val now = System.nanoTime()
        val previous = lastKeyframeRequestNs.get()
        if (now - previous >= 1_000_000_000L && lastKeyframeRequestNs.compareAndSet(previous, now)) {
            needsKeyframe = true
            sendJson(KEYFRAME, JSONObject().put("config_id", configId))
        } else {
            needsKeyframe = true
        }
    }
}
