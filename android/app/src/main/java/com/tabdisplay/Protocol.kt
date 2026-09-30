package com.tabdisplay

import java.io.EOFException
import java.io.InputStream
import java.nio.ByteBuffer

/** Pure v3 framing shared by the socket adapter and JVM tests. */
object Protocol {
    const val VERSION = 3
    const val HELLO = 1
    const val VIDEO = 2
    const val INPUT = 3
    const val CONFIG = 4
    const val PAIR_REQUIRED = 5
    const val PAIR = 6
    const val PAIRED = 7
    const val ERROR = 8
    const val RESIZE = 9
    const val PING = 10
    const val PONG = 11
    const val STATS = 12
    const val SCROLL = 13
    const val PROFILE = 14
    const val AUDIO = 15
    const val KEYFRAME = 16
    const val PAUSE = 17
    const val RESUME = 18
    const val TEXT = 19
    const val KEY = 20
    const val MOD_CTRL = 1
    const val MOD_SHIFT = 2
    const val MOD_ALT = 4
    const val MOD_META = 8
    const val MAX_MESSAGE = 16 shl 20
    const val MAX_CONTROL = 64 shl 10
    const val MAX_TEXT = 16 shl 10
    const val MAX_VIDEO = 8 shl 20
    const val MAX_AUDIO = 256 shl 10
    const val MAX_INPUT = 1 + 255 * 18
    const val MIN_DIMENSION = 16
    const val MAX_DIMENSION = 7680
    const val MAX_PIXELS = 16_777_216L
    const val MAX_FPS = 240

    data class Frame(val type: Int, val payload: ByteArray)

    fun frame(type: Int, payload: ByteArray): ByteArray {
        require(type in HELLO..KEY) { "unknown message type" }
        require(payload.size <= MAX_MESSAGE && payload.size <= payloadLimit(type)) { "message too large" }
        return ByteBuffer.allocate(5 + payload.size)
            .put(type.toByte())
            .putInt(payload.size)
            .put(payload)
            .array()
    }

    class Reader(private val input: InputStream) {
        fun read(): Frame {
            val type = input.read()
            if (type < 0) throw EOFException("missing message type")
            val header = ByteArray(4)
            readFully(header)
            val length = ByteBuffer.wrap(header).int
            require(type in HELLO..KEY) { "unknown message type" }
            require(length in 0..MAX_MESSAGE && length <= payloadLimit(type)) { "message too large" }
            val payload = ByteArray(length)
            readFully(payload)
            return Frame(type, payload)
        }

        private fun readFully(buffer: ByteArray) {
            var offset = 0
            while (offset < buffer.size) {
                val read = input.read(buffer, offset, buffer.size - offset)
                if (read < 0) throw EOFException("truncated message")
                if (read == 0) continue
                offset += read
            }
        }
    }

    fun validDimensions(width: Int, height: Int): Boolean =
        width >= MIN_DIMENSION && height >= MIN_DIMENSION &&
            width <= MAX_DIMENSION && height <= MAX_DIMENSION &&
            width % 16 == 0 && height % 16 == 0 && width.toLong() * height <= MAX_PIXELS

    fun validText(text: String): Boolean = text.toByteArray(Charsets.UTF_8).size <= MAX_TEXT

    private fun payloadLimit(type: Int): Int = when (type) {
        VIDEO -> MAX_VIDEO
        AUDIO -> MAX_AUDIO
        INPUT -> MAX_INPUT
        TEXT -> MAX_CONTROL
        KEY -> MAX_CONTROL
        else -> MAX_CONTROL
    }
}
