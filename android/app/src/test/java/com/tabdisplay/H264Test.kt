package com.tabdisplay

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class H264Test {
    @Test
    fun acceptsThreeAndFourByteAnnexBIdrStarts() {
        assertTrue(containsH264Idr(byteArrayOf(0, 0, 1, 0x65)))
        assertTrue(containsH264Idr(byteArrayOf(0, 0, 0, 1, 0x65)))
        assertTrue(containsH264Idr(byteArrayOf(0, 0, 1, 0x67, 1, 0, 0, 0, 1, 0x65, 2)))
        assertTrue(containsH264Idr(byteArrayOf(0, 0, 0, 1, 0x65, 2)))
        assertFalse(containsH264Idr(byteArrayOf(0, 0, 1, 0x41, 2)))
        assertFalse(containsH264Idr(byteArrayOf(1, 2, 3, 4)))
        assertFalse(containsH264Idr(byteArrayOf(0, 0, 0, 1)))
    }
}
