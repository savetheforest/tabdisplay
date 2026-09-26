package com.tabdisplay

import android.annotation.SuppressLint
import android.app.Activity
import android.app.AlertDialog
import android.graphics.Color
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build
import android.os.Bundle
import android.provider.Settings
import android.text.InputFilter
import android.text.InputType
import android.view.Gravity
import android.view.MotionEvent
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
                        setOnClickListener { showDisplay(pc.host, pc.id) }
                    }, width)
                }
            }
        }.also { it.start() }
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
        val view = SurfaceView(this)
        val container = FrameLayout(this).apply {
            setBackgroundColor(Color.BLACK)
            addView(view)
        }
        setContentView(container)
        // Letterbox: keep the PC's aspect ratio in any tablet orientation.
        var video = 0 to 0
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
                val hello = JSONObject()
                    .put("v", 2)
                    .put("device_id", deviceId)
                    .put("device_name", deviceName)
                    .put("token", pcId?.let { prefs.getString("token_$it", null) } ?: "")
                    .put("screen", JSONArray(listOf(bounds.width(), bounds.height())))
                    .put("decodable", JSONArray(listOf(w, h)))
                    .put("dpi", resources.displayMetrics.densityDpi)
                stream = Stream(host, holder.surface, hello, object : StreamEvents {
                    override fun onVideoSize(width: Int, height: Int) = runOnUiThread { video = width to height; fit() }
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

            override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) = Unit

            override fun surfaceDestroyed(holder: SurfaceHolder) {
                stream?.close()
                stream = null
            }
        })
        view.setOnTouchListener { v, e ->
            val action = when (e.actionMasked) {
                MotionEvent.ACTION_DOWN -> 0
                MotionEvent.ACTION_MOVE -> 1
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> 2
                else -> return@setOnTouchListener true
            }
            stream?.touch(action, e.x / v.width, e.y / v.height)
            true
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
            .setNegativeButton("Cancelar") { _, _ -> stream?.close(); stream = null; showConnect(null) }
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
}
