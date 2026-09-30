package com.tabdisplay

/** Small Annex-B scanner used only to gate recovery after decoder loss. */
fun containsH264Idr(accessUnit: ByteArray): Boolean {
    var i = 0
    while (i + 3 < accessUnit.size) {
        val start = when {
            accessUnit[i] == 0.toByte() && accessUnit[i + 1] == 0.toByte() && accessUnit[i + 2] == 1.toByte() -> 3
            accessUnit[i] == 0.toByte() && accessUnit[i + 1] == 0.toByte() && accessUnit[i + 2] == 0.toByte() && accessUnit[i + 3] == 1.toByte() -> 4
            else -> { i++; continue }
        }
        val nal = i + start
        if (nal < accessUnit.size && (accessUnit[nal].toInt() and 0x1f) == 5) return true
        i = nal + 1
    }
    return false
}
