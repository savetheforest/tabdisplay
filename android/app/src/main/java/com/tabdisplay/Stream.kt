package com.tabdisplay

import android.media.MediaCodec
import android.media.MediaFormat
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
private const val TOUCH = 3
private const val CONFIG = 4
private const val PAIR_REQUIRED = 5
private const val PAIR = 6
private const val PAIRED = 7
private const val ERROR = 8
private const val MAX_MSG = 16 shl 20

/** What a session reports back to the UI. Called from the stream thread. */
interface StreamEvents {
    fun onVideoSize(width: Int, height: Int)
    /** The PC shows a 6-digit code; answer with [Stream.pair]. `wrong` = the last code didn't match. */
    fun onPairRequired(pcName: String, wrong: Boolean)
    fun onPaired(pcId: String, token: String)
    fun onClose(reason: String)
}

/** One session with the PC: reads video into a MediaCodec rendering to [surface], sends touches back. */
class Stream(
    private val host: String,
    private val surface: Surface,
    private val hello: JSONObject,
    private val events: StreamEvents,
) {
    private val socket = Socket()
    private val sender = Executors.newSingleThreadExecutor()
    private val closed = AtomicBoolean(false)
    private val freeInputs = LinkedBlockingQueue<Int>()
    private lateinit var out: DataOutputStream
    @Volatile private var codec: MediaCodec? = null
    @Volatile private var connected = false
    /** Set when the PC explains why it ends the session (ERROR message). */
    @Volatile private var pcReason: String? = null

    fun start() = thread(name = "stream") {
        val reason = try {
            run()
            "O PC encerrou a conexão."
        } catch (e: Exception) {
            pcReason ?: friendly(e)
        }
        if (close()) events.onClose(reason)
    }

    private fun friendly(e: Exception): String = when {
        !connected && (e is SocketTimeoutException || e is ConnectException || e is NoRouteToHostException) ->
            "O PC não respondeu. O TabDisplay está aberto nele e na mesma rede?"
        connected && (e is EOFException || e is SocketException) -> "A conexão com o PC caiu."
        else -> "Erro: ${e.message ?: e.javaClass.simpleName}"
    }

    /** Returns true only for the call that actually closed the session. */
    fun close(): Boolean {
        if (!closed.compareAndSet(false, true)) return false
        runCatching { socket.close() }
        sender.shutdownNow()
        codec?.let { runCatching { it.stop(); it.release() } }
        codec = null
        return true
    }

    /** action: 0 down, 1 move, 2 up; x/y normalized to 0..1. */
    fun touch(action: Int, x: Float, y: Float) = send(TOUCH) {
        writeByte(action)
        writeFloat(x)
        writeFloat(y)
    }

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
            require(len in 0..MAX_MSG) { "Mensagem inválida do PC" }
            val payload = ByteArray(len)
            input.readFully(payload)
            when (type) {
                VIDEO -> decode(payload)
                CONFIG -> json(payload).let { startDecoder(it.getInt("width"), it.getInt("height")) }
                PAIR_REQUIRED -> json(payload).let { events.onPairRequired(it.optString("pc_name", "PC"), it.optBoolean("wrong")) }
                PAIRED -> json(payload).let { events.onPaired(it.getString("pc_id"), it.getString("token")) }
                ERROR -> pcReason = json(payload).optString("message", "O PC recusou a conexão.")
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
        events.onVideoSize(width, height)
        codec?.release()
        freeInputs.clear()
        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
            setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
        }
        codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC).apply {
            // Callbacks run on the main looper (this thread has none).
            setCallback(object : MediaCodec.Callback() {
                override fun onInputBufferAvailable(c: MediaCodec, index: Int) {
                    freeInputs.put(index)
                }

                override fun onOutputBufferAvailable(c: MediaCodec, index: Int, info: MediaCodec.BufferInfo) {
                    runCatching { c.releaseOutputBuffer(index, true) } // render immediately, no pacing
                }

                override fun onError(c: MediaCodec, e: MediaCodec.CodecException) {
                    runCatching { socket.close() } // unblocks the reader, which reports the error
                }

                override fun onOutputFormatChanged(c: MediaCodec, format: MediaFormat) = Unit
            })
            configure(format, surface, null, 0)
            start()
        }
    }

    private fun decode(frame: ByteArray) {
        val c = codec ?: return
        // ponytail: drops the frame if the decoder is stalled; can smear until the next keyframe.
        val index = freeInputs.poll(500, TimeUnit.MILLISECONDS) ?: return
        c.getInputBuffer(index)!!.apply {
            clear()
            put(frame)
        }
        c.queueInputBuffer(index, 0, frame.size, System.nanoTime() / 1000, 0)
    }
}
