package com.tabdisplay

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class SessionRetryTest {
    @Test
    fun backoffIsBoundedAndMonotonic() {
        assertEquals(0L, SessionRetry.delayMs(0))
        assertEquals(1_000L, SessionRetry.delayMs(1))
        assertEquals(10_000L, SessionRetry.delayMs(4))
        assertEquals(10_000L, SessionRetry.delayMs(99))
        assertEquals(0L, SessionRetry.delayMs(-1))
    }

    @Test
    fun identityAndAuthorizationFailuresNeedExplicitAction() {
        assertTrue(SessionRetry.allowsAutomaticRetry(null))
        assertTrue(SessionRetry.allowsAutomaticRetry("conexão caiu"))
        assertFalse(SessionRetry.allowsAutomaticRetry("identidade alterada"))
        assertFalse(SessionRetry.allowsAutomaticRetry("credencial revogada"))
        assertFalse(SessionRetry.allowsAutomaticRetry("TLS recusado"))
        assertFalse(SessionRetry.allowsAutomaticRetry("pareamento cancelado"))
    }
}
