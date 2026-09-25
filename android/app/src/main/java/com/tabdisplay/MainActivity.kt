package com.tabdisplay

import android.annotation.SuppressLint
import android.app.Activity
import android.graphics.Color
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Bundle
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

class MainActivity : Activity() {
    private var stream: Stream? = null
    private val prefs by lazy { getPreferences(MODE_PRIVATE) }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        showConnect(null)
    }

    private fun showConnect(message: String?) {
        val width = LinearLayout.LayoutParams((420 * resources.displayMetrics.density).toInt(), -2)
        val info = TextView(this).apply {
            text = message ?: "USB: no PC, clique em \"Conectar por USB\" antes."
            gravity = Gravity.CENTER
        }
        val ip = EditText(this).apply {
            hint = "IP do PC"
            setText(prefs.getString("ip", ""))
            isSingleLine = true
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
        }
        val wifi = Button(this).apply {
            text = "Conectar por Wi‑Fi"
            setOnClickListener {
                val host = ip.text.toString().trim()
                prefs.edit().putString("ip", host).apply()
                showDisplay(host)
            }
        }
        val usb = Button(this).apply {
            text = "Conectar por USB"
            setOnClickListener { showDisplay("127.0.0.1") } // PC ran `adb reverse`
        }
        setContentView(LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            gravity = Gravity.CENTER
            listOf(info, ip, wifi, usb).forEach { addView(it, width) }
        })
        window.insetsController?.show(WindowInsets.Type.systemBars()) // after setContentView: needs the decor view
    }

    @SuppressLint("ClickableViewAccessibility")
    private fun showDisplay(host: String) {
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
                val hello = intArrayOf(w, h, resources.displayMetrics.densityDpi)
                val onVideoSize = { w: Int, h: Int -> runOnUiThread { video = w to h; fit() } }
                stream = Stream(host, holder.surface, hello, onVideoSize) { reason ->
                    runOnUiThread {
                        stream = null
                        showConnect(reason)
                    }
                }.also { it.start() }
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
