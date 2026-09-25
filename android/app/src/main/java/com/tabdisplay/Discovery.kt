package com.tabdisplay

import android.content.Context
import android.net.wifi.WifiManager
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketTimeoutException
import kotlin.concurrent.thread

private const val BEACON_PORT = 7071
private const val USB_HOST = "127.0.0.1"
private const val FORGET_AFTER_MS = 8000L // broadcasts arrive late on Wi-Fi in power save

/**
 * Finds PCs to connect to: Wi-Fi ones from their UDP broadcast beacon ("TABDISPLAY <name>" on port 7071,
 * see PROTOCOL.md), and USB when `adb reverse` makes 127.0.0.1:7070 reachable. Calls [onChange] with the
 * current list whenever it changes, from a background thread.
 */
class Discovery(context: Context, private val onChange: (List<Pc>) -> Unit) {
    data class Pc(val name: String, val host: String) {
        val usb get() = host == USB_HOST
    }

    // Some Wi-Fi drivers drop broadcast packets in power save unless an app holds this lock.
    private val lock = (context.applicationContext.getSystemService(Context.WIFI_SERVICE) as WifiManager)
        .createMulticastLock("tabdisplay").apply { setReferenceCounted(false) }
    private val socket = DatagramSocket(null).apply {
        reuseAddress = true
        broadcast = true
        soTimeout = 1000
        bind(InetSocketAddress(BEACON_PORT))
    }
    private val seen = LinkedHashMap<String, Pair<Pc, Long>>() // by host
    private var published = emptyList<Pc>()
    @Volatile private var running = true

    fun start() {
        lock.acquire()
        thread(name = "discovery") { listen() }
        thread(name = "usb-probe") { probeUsb() }
    }

    fun stop() {
        running = false
        socket.close()
        lock.release()
    }

    private fun listen() {
        val buf = ByteArray(256)
        while (running) {
            try {
                val packet = DatagramPacket(buf, buf.size)
                socket.receive(packet)
                val text = String(packet.data, 0, packet.length)
                if (text.startsWith("TABDISPLAY ")) {
                    see(Pc(text.removePrefix("TABDISPLAY ").trim(), packet.address.hostAddress ?: continue))
                }
            } catch (_: SocketTimeoutException) {
            } catch (_: Exception) {
                if (!running) return
            }
            publish()
        }
    }

    // ponytail: a USB probe opens a real connection to the PC every 2s; fine while the connect screen is up.
    private fun probeUsb() {
        while (running) {
            val reachable = runCatching { Socket().use { it.connect(InetSocketAddress(USB_HOST, 7070), 300) } }.isSuccess
            if (reachable) see(Pc("USB", USB_HOST))
            publish()
            Thread.sleep(2000)
        }
    }

    private fun see(pc: Pc) = synchronized(seen) { seen[pc.host] = pc to System.currentTimeMillis() }

    private fun publish() {
        val changed = synchronized(seen) {
            val now = System.currentTimeMillis()
            seen.values.removeAll { now - it.second > FORGET_AFTER_MS }
            val list = seen.values.map { it.first }.sortedBy { !it.usb }
            if (list == published) null else list.also { published = it }
        }
        if (changed != null && running) onChange(changed)
    }
}
