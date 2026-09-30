package com.tabdisplay

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

class AudioTest {
    @Test
    fun decodesVersionedSequenceAndPts() {
        val payload = ByteBuffer.allocate(17 + 3).order(ByteOrder.BIG_ENDIAN)
            .put(1).putLong(7L).putLong(960L).put(byteArrayOf(1, 2, 3)).array()
        val packet = AudioPacket.decode(payload)
        assertEquals(7L, packet.sequence)
        assertEquals(960L, packet.ptsSamples)
        assertArrayEquals(byteArrayOf(1, 2, 3), packet.opus)
    }

    @Test
    fun keepsLegacyRawOpusPacket() {
        val packet = AudioPacket.decode(byteArrayOf(0x4f, 0x70, 0x75, 0x73))
        assertEquals(null, packet.sequence)
        assertEquals(null, packet.ptsSamples)
        assertArrayEquals(byteArrayOf(0x4f, 0x70, 0x75, 0x73), packet.opus)
    }
}
