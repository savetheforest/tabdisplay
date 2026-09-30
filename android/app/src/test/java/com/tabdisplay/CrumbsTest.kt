package com.tabdisplay

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import java.net.SocketException
import org.junit.Test

class CrumbsTest {
    @Test
    fun localExportRedactsSentinelSecretsAndAddresses() {
        Crumbs.add("test", "token=sentinel 192.168.1.42")
        val export = Crumbs.exportText()
        assertFalse(export.contains("sentinel"))
        assertFalse(export.contains("192.168.1.42"))
        assertTrue(export.contains("<redacted>"))
        assertTrue(export.contains("<address>"))
    }

    @Test
    fun localExportKeepsOnlyTheBoundedRecentWindow() {
        repeat(70) { Crumbs.add("window", "event-$it") }
        val export = Crumbs.exportText()
        assertTrue(export.contains("eventos recentes: 64"))
        assertFalse(export.contains("event-0"))
        assertTrue(export.contains("event-69"))
    }

    @Test
    fun ordinaryNetworkErrorsAreNotUnexpectedDiagnostics() {
        assertFalse(Crumbs.unexpected(SocketException("closed")))
        assertTrue(Crumbs.unexpected(IllegalStateException("decoder")))
    }
}
