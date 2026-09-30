package com.tabdisplay

import java.io.ByteArrayInputStream
import java.nio.ByteBuffer
import java.nio.charset.StandardCharsets
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ProtocolFixturesTest {
    private fun fixture(name: String): ByteArray = checkNotNull(javaClass.getResourceAsStream("/v3/$name")).readBytes()

    private fun hex(name: String): ByteArray = fixture(name).toString(StandardCharsets.UTF_8)
        .trim().split(Regex("\\s+")).map { it.toInt(16).toByte() }.toByteArray()

    @Test
    fun framingUsesSharedHelloAndConfigFixtures() {
        val hello = fixture("hello.json")
        val config = fixture("config.json")
        val encoded = Protocol.frame(Protocol.HELLO, hello) + Protocol.frame(Protocol.CONFIG, config)
        val reader = Protocol.Reader(ByteArrayInputStream(encoded))
        val helloFrame = reader.read()
        assertEquals(Protocol.HELLO, helloFrame.type)
        assertArrayEquals(hello, helloFrame.payload)
        val configFrame = reader.read()
        assertEquals(Protocol.CONFIG, configFrame.type)
        assertArrayEquals(config, configFrame.payload)
    }

    @Test
    fun framingRejectsTruncationAndOversizedPayloads() {
        assertThrows<Exception> { Protocol.Reader(ByteArrayInputStream(byteArrayOf(1, 0, 0, 0))).read() }
        assertThrows<Exception> { Protocol.Reader(ByteArrayInputStream(byteArrayOf(1, 0x01, 0, 0, 0))).read() }
        assertThrows<IllegalArgumentException> { Protocol.frame(Protocol.VIDEO, ByteArray(Protocol.MAX_MESSAGE + 1)) }
        assertThrows<IllegalArgumentException> { Protocol.frame(Protocol.INPUT, ByteArray(Protocol.MAX_INPUT + 1)) }
        assertThrows<IllegalArgumentException> { Protocol.Reader(ByteArrayInputStream(byteArrayOf(99, 0, 0, 0, 0))).read() }
        assertThrows<Exception> { Protocol.Reader(ByteArrayInputStream(byteArrayOf(Protocol.CONFIG.toByte(), 0x01, 0, 0, 0))).read() }
    }

    @Test
    fun inputFixtureRoundTripsAndRejectsInvalidNumbers() {
        val expected = hex("input-touch-pen.hex")
        val contacts = InputFrame.parse(expected)
        assertNotNull(contacts)
        assertEquals(3, contacts!!.size)
        assertEquals(-20, contacts[2].tiltX)
        assertArrayEquals(expected, InputFrame.encode(contacts))

        val nan = expected.copyOf()
        ByteBuffer.wrap(nan, 5, 4).putFloat(Float.NaN)
        assertNull(InputFrame.parse(nan))
        val unknown = expected.copyOf()
        unknown[3] = 9
        assertNull(InputFrame.parse(unknown))

        val scroll = ByteBuffer.wrap(hex("scroll.hex"))
        assertEquals(0.5f, scroll.float, 0f)
        assertEquals(0.25f, scroll.float, 0f)
        assertEquals(-1f, scroll.float, 0f)
        assertEquals(3f, scroll.float, 0f)
        assertTrue(Protocol.validDimensions(1280, 800))
        assertFalse(Protocol.validDimensions(0, 800))
        assertFalse(Protocol.validDimensions(4112, 4096))
    }

    @Test
    fun textFramesAndUtf8LimitAreBounded() {
        val text = "linha 1\nlinha 2 — café"
        val frame = Protocol.frame(Protocol.TEXT, text.toByteArray(StandardCharsets.UTF_8))
        val reader = Protocol.Reader(ByteArrayInputStream(frame)).read()
        assertEquals(Protocol.TEXT, reader.type)
        assertEquals(text, String(reader.payload, StandardCharsets.UTF_8))
        assertTrue(Protocol.validText("é".repeat(Protocol.MAX_TEXT / 2)))
        assertFalse(Protocol.validText("é".repeat(Protocol.MAX_TEXT / 2 + 1)))
        assertTrue(Protocol.validText(""))
    }

    private inline fun <reified T : Throwable> assertThrows(block: () -> Unit) {
        try {
            block()
        } catch (error: Throwable) {
            if (error is T) return
            throw AssertionError("expected ${T::class.java.name}, got ${error::class.java.name}", error)
        }
        throw AssertionError("expected ${T::class.java.name}")
    }
}
