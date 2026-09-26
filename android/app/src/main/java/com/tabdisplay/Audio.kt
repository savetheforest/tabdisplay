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
    @Volatile var muted = false
        set(value) {
            field = value
            track.setVolume(if (value) 0f else 1f)
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

    /** Decodes one Opus packet and queues the PCM for playback. */
    fun play(packet: ByteArray) {
        val i = codec.dequeueInputBuffer(0)
        if (i >= 0) {
            codec.getInputBuffer(i)!!.apply { clear(); put(packet) }
            codec.queueInputBuffer(i, 0, packet.size, pts, 0)
            pts += 20_000
        }
        while (true) {
            val o = codec.dequeueOutputBuffer(info, 0)
            if (o < 0) break // also covers INFO_OUTPUT_FORMAT_CHANGED etc.: nothing to write
            codec.getOutputBuffer(o)?.let { track.write(it, info.size, AudioTrack.WRITE_NON_BLOCKING) }
            codec.releaseOutputBuffer(o, false)
        }
    }

    fun release() {
        runCatching { track.stop(); track.release() }
        runCatching { codec.stop(); codec.release() }
    }
}
