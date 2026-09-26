package com.tabdisplay

import io.sentry.Breadcrumb
import io.sentry.Sentry
import java.io.EOFException
import java.net.ConnectException
import java.net.NoRouteToHostException
import java.net.SocketException
import java.net.SocketTimeoutException

/** Connection-flow breadcrumbs for crash reports. No-ops when Sentry isn't running; never put tokens, names or addresses in here. */
object Crumbs {
    fun add(category: String, message: String) {
        Sentry.addBreadcrumb(Breadcrumb().apply { this.category = category; this.message = message })
    }

    /** Ordinary network trouble is shown to the user, not reported; anything else (TLS failures, bugs) is. */
    fun unexpected(e: Exception): Boolean =
        e !is SocketException && e !is SocketTimeoutException && e !is ConnectException && e !is NoRouteToHostException && e !is EOFException && e !is SecurityException
}
