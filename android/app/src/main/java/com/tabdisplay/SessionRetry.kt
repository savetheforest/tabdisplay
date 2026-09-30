package com.tabdisplay

/** Pure retry policy; lifecycle and Handler ownership remain in MainActivity. */
object SessionRetry {
    private val delaysMs = listOf(0L, 1_000L, 2_000L, 5_000L, 10_000L)

    fun delayMs(attempts: Int): Long = delaysMs[attempts.coerceAtLeast(0).coerceAtMost(delaysMs.lastIndex)]

    /** Identity, protocol and authorization failures require an explicit user action. */
    fun allowsAutomaticRetry(reason: String?): Boolean = reason == null ||
        !reason.contains(Regex("identidade|atualize|credencial|pareamento|TLS", RegexOption.IGNORE_CASE))
}
