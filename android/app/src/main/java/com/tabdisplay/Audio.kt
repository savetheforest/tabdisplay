package com.tabdisplay

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.media.MediaCodec
import android.media.MediaFormat
import java.nio.ByteBuffer
import java.nio.ByteOrder

private const val RATE = 48_000
private const val CHANNELS = 2
private const val AUDIO_HEADER_VERSION = 1
private const val AUDIO_HEADER_SIZE = 17

data class AudioPacket(val sequence: Long?, val ptsSamples: Long?, val opus: ByteArray) {
    companion object {
        fun decode(payload: ByteArray): AudioPacket {
            if (payload.size >= AUDIO_HEADER_SIZE && payload[0].toInt() == AUDIO_HEADER_VERSION) {
                val header = ByteBuffer.wrap(payload).order(ByteOrder.BIG_ENDIAN)
                header.get()
                return AudioPacket(header.long, header.long, payload.copyOfRange(AUDIO_HEADER_SIZE, payload.size))
            }
            // Legacy v3 clients received raw Opus packets; keep mixed-version upgrades safe.
            return AudioPacket(null, null, payload)
        }
    }
}

/**
 * Plays the PC's audio (AUDIO messages, PROTOCOL.md): Opus packets -> MediaCodec -> AudioTrack.
 * Everything runs on the caller's thread (the stream reader). The track's buffer is small (100 ms) and writes
 * never block, so if playback falls behind, samples are dropped instead of turning into growing delay.
 */
class Audio {
    private val codec: MediaCodec
    private val track: AudioTrack
    private val info = MediaCodec.BufferInfo()
    private var pts = 0L
    private var lastSequence: Long? = null
    @Volatile var muted = false
        set(value) {
            field = value
            track.setVolume(if (value) 0f else 1f)
            if (value) runCatching { track.pause(); track.flush(); track.play() }
        }

    init {
        // Opus in MediaCodec wants the OpusHead as csd-0, plus codec delay and seek pre-roll (ns) as csd-1/2.
        val head = ByteBuffer.allocate(19).order(ByteOrder.LITTLE_ENDIAN)
            .put("OpusHead".toByteArray()).put(1).put(CHANNELS.toByte())
            .putShort(312).putInt(RATE).putShort(0).put(0).flip() as ByteBuffer
        val nanos = { ns: Long -> ByteBuffer.allocate(8).order(ByteOrder.nativeOrder()).putLong(ns).flip() as ByteBuffer }
        val format = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_OPUS, RATE, CHANNELS).apply {
            setByteBuffer("csd-0", head)
            setByteBuffer("csd-1", nanos(6_500_000))
            setByteBuffer("csd-2", nanos(80_000_000))
        }
        codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_AUDIO_OPUS).apply {
            configure(format, null, null, 0)
            start()
        }
        val pcm = AudioFormat.Builder()
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(RATE).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).build()
        track = AudioTrack.Builder()
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setAudioFormat(pcm)
            .setTransferMode(AudioTrack.MODE_STREAM)
            .setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
            .setBufferSizeInBytes(maxOf(AudioTrack.getMinBufferSize(RATE, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT), RATE * 4 / 10))
            .build()
        track.play()
    }

    /** Decodes one Opus packet and queues the PCM. A sequence gap flushes stale PCM. */
    fun play(packet: AudioPacket) {
        val sequence = packet.sequence
        if (sequence != null && lastSequence != null && sequence != lastSequence!! + 1L) {
            runCatching { codec.flush(); track.pause(); track.flush(); track.play() }
        }
        lastSequence = sequence ?: lastSequence?.plus(1L)
        val inputPts = packet.ptsSamples?.let { it * 1_000_000L / RATE } ?: pts
        val i = codec.dequeueInputBuffer(0)
        if (i >= 0) {
            val input = codec.getInputBuffer(i) ?: return
            input.clear()
            input.put(packet.opus)
            codec.queueInputBuffer(i, 0, packet.opus.size, inputPts, 0)
            pts = inputPts + 20_000
        }
        while (true) {
            val o = codec.dequeueOutputBuffer(info, 0)
            if (o == MediaCodec.INFO_TRY_AGAIN_LATER) break
            if (o == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) continue
            if (o < 0) break
            val output = codec.getOutputBuffer(o)
            if (output != null && info.size > 0) {
                val start = info.offset.coerceAtLeast(0)
                val end = (start + info.size).coerceAtMost(output.limit())
                if (end > start) {
                    val bytes = ByteArray(end - start)
                    output.duplicate().apply { position(start); limit(end); get(bytes) }
                    var offset = 0
                    while (offset < bytes.size) {
                        val written = track.write(bytes, offset, bytes.size - offset, AudioTrack.WRITE_NON_BLOCKING)
                        if (written <= 0) {
                            if (written == AudioTrack.ERROR_DEAD_OBJECT) throw IllegalStateException("AudioTrack encerrado")
                            break
                        }
                        offset += written
                    }
                }
            }
            codec.releaseOutputBuffer(o, false)
        }
    }

    fun release() {
        runCatching { track.pause(); track.flush(); track.stop(); track.release() }
        runCatching { codec.stop(); codec.release() }
    }
}
