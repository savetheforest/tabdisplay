package com.tabdisplay

/** A decoder-supported upper bound for one exact frame-rate bucket. */
data class VideoMode(val width: Int, val height: Int, val fps: Int)

/** The concrete decoder and the finite modes discovered for that decoder. */
data class DecoderCapability(
    val name: String,
    val canonicalName: String,
    val hardwareAccelerated: Boolean,
    val softwareOnly: Boolean,
    val modes: List<VideoMode>,
)

/** Pure selection rules; Android codec enumeration stays in the platform adapter. */
object VideoNegotiation {
    val standardFps = listOf(120, 90, 60, 30)

    fun select(candidates: List<DecoderCapability>): DecoderCapability? = candidates
        .filter { it.modes.isNotEmpty() }
        .maxWithOrNull(compareBy<DecoderCapability> { it.hardwareAccelerated }
            .thenBy { it.modes.maxOf { mode -> mode.width.toLong() * mode.height } }
            .thenBy { it.modes.maxOf(VideoMode::fps) })

    fun modeFor(capability: DecoderCapability, width: Int, height: Int, fps: Int): VideoMode? {
        val eligible = capability.modes.filter { it.fps <= fps }
        val fitting = eligible.filter { contains(it, width, height) }
        return (fitting.ifEmpty { eligible }).maxWithOrNull(compareBy<VideoMode> { it.fps }
            .thenBy { it.width.toLong() * it.height })
    }

    fun contains(mode: VideoMode, width: Int, height: Int): Boolean =
        (mode.width >= width && mode.height >= height) || (mode.width >= height && mode.height >= width)
}
