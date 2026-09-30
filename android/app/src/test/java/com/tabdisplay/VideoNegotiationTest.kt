package com.tabdisplay

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

class VideoNegotiationTest {
    @Test
    fun selectsConcreteHardwareDecoderAndFiniteModes() {
        val software = DecoderCapability("sw", "sw", false, true, listOf(VideoMode(1920, 1080, 60)))
        val hardware = DecoderCapability(
            "hw.avc", "hw.avc", true, false,
            listOf(VideoMode(1920, 1080, 60), VideoMode(1280, 720, 90)),
        )
        val selected = VideoNegotiation.select(listOf(software, hardware))
        assertNotNull(selected)
        assertEquals("hw.avc", selected!!.name)
        assertEquals(VideoMode(1920, 1080, 60), VideoNegotiation.modeFor(selected, 1920, 1080, 60))
        assertEquals(VideoMode(1280, 720, 90), VideoNegotiation.modeFor(selected, 1280, 720, 120))
    }

    @Test
    fun doesNotInventNinetyFpsWhenOnlySixtyIsAdvertised() {
        val capability = DecoderCapability("hw.avc", "hw.avc", true, false, listOf(VideoMode(1920, 1080, 60)))
        assertNull(VideoNegotiation.modeFor(capability, 1920, 1080, 30))
        assertEquals(60, VideoNegotiation.modeFor(capability, 1920, 1080, 120)!!.fps)
    }
}
