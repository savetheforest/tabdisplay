package com.tabdisplay

import android.media.MediaCodec
import android.media.MediaFormat
import android.view.Surface
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.nio.ByteBuffer
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.concurrent.thread

// Wire protocol: see PROTOCOL.md at the repo root.
private const val PORT = 7070
private const val HELLO = 1
private const val VIDEO = 2
private const val TOUCH = 3
private const val CONFIG = 4
private const val MAX_MSG = 16 shl 20

/** One session with the PC: reads video into a MediaCodec rendering to [surface], sends touches back. */
class Stream(
    private val host: String,
    private val surface: Surface,
    private val hello: IntArray, // width, height, dpi
    private val onVideoSize: (width: Int, height: Int) -> Unit,
    private val onClose: (reason: String) -> Unit,
) {
    private val socket = Socket()
    private val sender = Executors.newSingleThreadExecutor()
    private val closed = AtomicBoolean(false)
    private val freeInputs = LinkedBlockingQueue<Int>()
    private lateinit var out: DataOutputStream
    @Volatile private var codec: MediaCodec? = null

    fun start() = thread(name = "stream") {
        val reason = try {
            run()
            "Desconectado"
        } catch (e: Exception) {
            e.message ?: e.toString()
        }
        if (close()) onClose(reason)
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

    private fun run() {
        socket.tcpNoDelay = true
        socket.connect(InetSocketAddress(host, PORT), 3000)
        out = DataOutputStream(BufferedOutputStream(socket.getOutputStream()))
        send(HELLO) { hello.forEach(::writeInt) }

        val input = DataInputStream(BufferedInputStream(socket.getInputStream(), 1 shl 16))
        while (!closed.get()) {
            val type = input.readUnsignedByte()
            val len = input.readInt()
            require(len in 0..MAX_MSG) { "Mensagem inválida do PC" }
            val payload = ByteArray(len)
            input.readFully(payload)
            when (type) {
                CONFIG -> startDecoder(ByteBuffer.wrap(payload).getInt(0), ByteBuffer.wrap(payload).getInt(4))
                VIDEO -> decode(payload)
            }
        }
    }

    private fun send(type: Int, body: DataOutputStream.() -> Unit) {
        if (closed.get()) return
        val bytes = ByteArrayOutputStream().also { DataOutputStream(it).body() }
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
        onVideoSize(width, height)
        codec?.release()
        freeInputs.clear()
        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
            setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
            setInteger(MediaFormat.KEY_MAX_INPUT_SIZE, width * height)
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
