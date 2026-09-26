package com.tabdisplay

import android.annotation.SuppressLint
import android.app.Activity
import android.app.AlertDialog
import android.graphics.Color
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.text.InputFilter
import android.text.InputType
import android.view.Gravity
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import android.widget.Button
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

class MainActivity : Activity() {
    private var stream: Stream? = null
    private var discovery: Discovery? = null
    private val prefs by lazy { getPreferences(MODE_PRIVATE) }
    private val handler = Handler(Looper.getMainLooper())
    /** Reconnect to the last PC when it shows up; off after the user disconnects on purpose. */
    private var autoConnect = true
    private var attempts = 0
    private var pendingRetry: Runnable? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        showConnect(null)
    }

    private fun showConnect(message: String?) {
        val width = LinearLayout.LayoutParams((420 * resources.displayMetrics.density).toInt(), -2)
        val info = TextView(this).apply {
            text = message ?: "Procurando PCs com o TabDisplay aberto…"
            gravity = Gravity.CENTER
        }
        // One button per PC found; rebuilt only when the list changes so typing an IP isn't interrupted.
        val pcs = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        val ip = EditText(this).apply {
            hint = "Ou digite o IP do PC"
            setText(prefs.getString("ip", ""))
            isSingleLine = true
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
        }
        val manual = Button(this).apply {
            text = "Conectar pelo IP"
            setOnClickListener {
                val host = ip.text.toString().trim()
                prefs.edit().putString("ip", host).apply()
                showDisplay(host, prefs.getString("pc_at_$host", null))
            }
        }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            listOf(info, pcs, ip, manual).forEach { addView(it, width) }
        })
        window.insetsController?.show(WindowInsets.Type.systemBars()) // after setContentView: needs the decor view

        discovery?.stop()
        discovery = Discovery(this) { found ->
            runOnUiThread {
                pcs.removeAllViews()
                for (pc in found) {
                    pcs.addView(Button(this).apply {
                        text = if (pc.usb) "USB (cabo)" else "${pc.name}  ·  Wi‑Fi ${pc.host}"
                        setOnClickListener {
                            autoConnect = true
                            showDisplay(pc.host, pc.id)
                        }
                    }, width)
                }
                maybeReconnect(found, info)
            }
        }.also { it.start() }
    }

    /** Reconnects to the last PC once discovery sees it again, waiting longer after each failure. */
    private fun maybeReconnect(found: List<Discovery.Pc>, info: TextView) {
        if (!autoConnect || pendingRetry != null) return
        val last = prefs.getString("last_pc", null) ?: return
        val pc = found.firstOrNull { if (last == "usb") it.usb else it.id == last } ?: return
        val delay = RETRY_DELAYS_MS[attempts.coerceAtMost(RETRY_DELAYS_MS.lastIndex)]
        if (delay > 0) info.text = "${info.text}\nReconectando a ${pc.name} em ${delay / 1000} s…"
        pendingRetry = Runnable {
            pendingRetry = null
            attempts++
            showDisplay(pc.host, pc.id)
        }.also { handler.postDelayed(it, delay) }
    }

    override fun onDestroy() {
        discovery?.stop()
        super.onDestroy()
    }

    @SuppressLint("ClickableViewAccessibility")
    /** Connects to the PC at [host]; [pcId] (from its beacon, or remembered) selects the pairing token. */
    private fun showDisplay(host: String, pcId: String?) {
        discovery?.stop()
        discovery = null
        pendingRetry?.let(handler::removeCallbacks)
        pendingRetry = null
        prefs.edit().putString("last_pc", pcId ?: if (host == "127.0.0.1") "usb" else null).apply()
        val view = SurfaceView(this)
        val container = FrameLayout(this).apply {
            setBackgroundColor(Color.BLACK)
            addView(view)
        }
        setContentView(container)
        // Letterbox: keep the PC's aspect ratio in any tablet orientation.
        var video = 0 to 0
        var sentSize = 0 to 0
        fun fit() {
            val (w, h) = video
            if (w == 0 || container.width == 0) return
            val scale = minOf(container.width.toFloat() / w, container.height.toFloat() / h)
            view.layoutParams = FrameLayout.LayoutParams((w * scale).toInt(), (h * scale).toInt(), Gravity.CENTER)
        }
        container.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ -> container.post(::fit) }
        window.insetsController?.apply {
            hide(WindowInsets.Type.systemBars())
            systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
        }
        // Surface lifetime == session lifetime: leaving the app disconnects, coming back reconnects.
        view.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                val bounds = windowManager.currentWindowMetrics.bounds
                val (w, h) = decodableSize(bounds.width(), bounds.height())
                sentSize = w to h
                val hello = JSONObject()
                    .put("v", 2)
                    .put("device_id", deviceId)
                    .put("device_name", deviceName)
                    .put("token", pcId?.let { prefs.getString("token_$it", null) } ?: "")
                    .put("screen", JSONArray(listOf(bounds.width(), bounds.height())))
                    .put("decodable", JSONArray(listOf(w, h)))
                    .put("dpi", resources.displayMetrics.densityDpi)
                stream = Stream(host, holder.surface, hello, object : StreamEvents {
                    override fun onVideoSize(width: Int, height: Int) = runOnUiThread {
                        attempts = 0 // the session works: next drop retries quickly again
                        video = width to height
                        fit()
                    }
                    override fun onPairRequired(pcName: String, wrong: Boolean) = runOnUiThread { askCode(pcName, wrong) }
                    override fun onPaired(pcId: String, token: String) {
                        prefs.edit().putString("token_$pcId", token).putString("pc_at_$host", pcId).apply()
                    }
                    override fun onClose(reason: String) = runOnUiThread {
                        stream = null
                        showConnect(reason)
                    }
                }).also { it.start() }
            }

            // Rotation: the window's shape changed, so ask the PC for a monitor of the new shape.
            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                val bounds = windowManager.currentWindowMetrics.bounds
                val size = decodableSize(bounds.width(), bounds.height())
                if (size != sentSize) {
                    sentSize = size
                    stream?.resize(size.first, size.second)
                }
            }

            override fun surfaceDestroyed(holder: SurfaceHolder) {
                stream?.close()
                stream = null
            }
        })
        // Every finger and the pen (including hovering) go to the PC as one INPUT frame per event.
        view.setOnTouchListener { v, e ->
            Input.encode(e, v.width, v.height)?.let { stream?.input(it) }
            true
        }
        view.setOnGenericMotionListener { v, e ->
            Input.encode(e, v.width, v.height)?.let { stream?.input(it) } != null
        }
    }

    private val deviceId: String
        get() = prefs.getString("device_id", null) ?: UUID.randomUUID().toString().also {
            prefs.edit().putString("device_id", it).apply()
        }

    private val deviceName: String
        get() = Settings.Global.getString(contentResolver, Settings.Global.DEVICE_NAME) ?: Build.MODEL

    /** The PC shows a 6-digit code the first time this tablet connects over Wi-Fi. */
    private fun askCode(pcName: String, wrong: Boolean) {
        val field = EditText(this).apply {
            inputType = InputType.TYPE_CLASS_NUMBER
            hint = "000000"
            filters = arrayOf(InputFilter.LengthFilter(6))
        }
        AlertDialog.Builder(this)
            .setTitle(if (wrong) "Código errado, tente de novo" else "Parear com $pcName")
            .setMessage("Digite o código de 6 dígitos que apareceu no TabDisplay do PC.")
            .setView(field)
            .setCancelable(false)
            .setPositiveButton("Parear") { _, _ -> stream?.pair(field.text.toString()) }
            .setNegativeButton("Cancelar") { _, _ ->
                autoConnect = false
                stream?.close()
                stream = null
                showConnect(null)
            }
            .show()
        field.requestFocus()
    }

    /**
     * Largest size with the screen's aspect ratio that the H.264 decoder handles at 60 fps.
     * The Redmi Pad 2 screen is 2560x1600 but its decoder tops out at 2560x1440, so this gives 2304x1440
     * and the SurfaceView scales it up.
     */
    private fun decodableSize(width: Int, height: Int): Pair<Int, Int> {
        val caps = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos
            .filter { !it.isEncoder && MediaFormat.MIMETYPE_VIDEO_AVC in it.supportedTypes }
            .map { it.getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC).videoCapabilities }
        for (percent in 100 downTo 25) {
            val w = width * percent / 100 and 15.inv()
            val h = height * percent / 100 and 15.inv()
            if (caps.any { it.areSizeAndRateSupported(w, h, 60.0) }) return w to h
        }
        return 1280 to 720
    }

    private companion object {
        val RETRY_DELAYS_MS = listOf(0L, 1000L, 2000L, 5000L, 10000L)
    }
}
