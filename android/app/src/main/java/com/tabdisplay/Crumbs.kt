package com.tabdisplay

import io.sentry.Breadcrumb
import io.sentry.Sentry
import java.io.EOFException
import java.net.ConnectException
import java.net.NoRouteToHostException
import java.net.SocketException
import java.net.SocketTimeoutException
import java.util.ArrayDeque

/** Connection-flow breadcrumbs for crash reports. No-ops when Sentry isn't running; never put tokens, names or addresses in here. */
object Crumbs {
    private const val MAX_RECENT = 64
    private val recent = ArrayDeque<String>()

    fun add(category: String, message: String) {
        val safe = sanitize(message)
        synchronized(recent) {
            recent.addLast("$category: $safe")
            while (recent.size > MAX_RECENT) recent.removeFirst()
        }
        Sentry.addBreadcrumb(Breadcrumb().apply { this.category = category; this.message = safe })
    }

    /** Sanitized local export: callers only add event classes/counts, never tokens, names or addresses. */
    fun exportText(): String = synchronized(recent) {
        buildString {
            appendLine("TabDisplay diagnóstico local")
            appendLine("eventos recentes: ${recent.size}")
            recent.forEach { appendLine(it) }
        }
    }

    private fun sanitize(message: String): String = message
        .replace(Regex("(?i)(token|code|password)\\s*[:=]\\s*\\S+"), "$1=<redacted>")
        .replace(Regex("\\b(?:\\d{1,3}\\.){3}\\d{1,3}\\b"), "<address>")

    /** Ordinary network trouble is shown to the user, not reported; anything else (TLS failures, bugs) is. */
    fun unexpected(e: Exception): Boolean =
        e !is SocketException && e !is SocketTimeoutException && e !is ConnectException && e !is NoRouteToHostException && e !is EOFException && e !is SecurityException
}
