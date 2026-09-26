package com.tabdisplay

import android.annotation.SuppressLint
import android.content.Context
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.Settings
import android.view.SurfaceHolder
import android.view.SurfaceView
import android.view.WindowInsets
import android.view.WindowInsetsController
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import org.json.JSONArray
import org.json.JSONObject
import java.util.UUID

private sealed interface Screen {
    /** [message] explains why we're here (connection dropped, PC refused…); null on a fresh start. */
    data class Connect(val message: String?) : Screen
    data class Display(val host: String, val pcId: String?, val name: String) : Screen
}

private data class PairRequest(val pcName: String, val wrong: Boolean)

class MainActivity : ComponentActivity() {
    private val prefs by lazy { getPreferences(MODE_PRIVATE) }
    private val handler = Handler(Looper.getMainLooper())

    // UI state
    private var screen by mutableStateOf<Screen>(Screen.Connect(null))
    private var pcs by mutableStateOf(emptyList<Discovery.Pc>())
    private var pairing by mutableStateOf<PairRequest?>(null)
    private var reconnecting by mutableStateOf<String?>(null)
    private var video by mutableStateOf<Pair<Int, Int>?>(null)
    private var stats by mutableStateOf<String?>(null)
    private var showStats by mutableStateOf(false)

    private var stream: Stream? = null
    private var discovery: Discovery? = null
    /** Reconnect to the last PC when it shows up; off after the user disconnects on purpose. */
    private var autoConnect = true
    private var attempts = 0
    private var pendingRetry: Runnable? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        showStats = prefs.getBoolean("show_stats", false)
        setContent {
            TabDisplayTheme {
                when (val s = screen) {
                    is Screen.Connect -> ConnectScreen(s.message)
                    is Screen.Display -> DisplayScreen(s)
                }
                pairing?.let { PairDialog(it) }
            }
        }
    }

    override fun onDestroy() {
        stopDiscovery()
        super.onDestroy()
    }

    // ---- connect screen ------------------------------------------------------------------------------

    @Composable
    private fun ConnectScreen(message: String?) {
        DisposableEffect(Unit) {
            startDiscovery()
            onDispose { stopDiscovery() }
        }
        LaunchedEffect(Unit) { window.insetsController?.show(WindowInsets.Type.systemBars()) }
        var manual by remember { mutableStateOf(false) }

        // Surface (not a plain background) so text gets the theme's onSurface color.
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
        Box(Modifier.fillMaxSize().safeDrawingPadding(), contentAlignment = Alignment.TopCenter) {
            Column(
                Modifier.widthIn(max = 640.dp).fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 32.dp, vertical = 40.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text("TabDisplay", style = MaterialTheme.typography.displaySmall, fontWeight = FontWeight.SemiBold)
                Text(
                    "Use este tablet como segundo monitor do seu PC.",
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(12.dp))

                message?.let { Notice(it, MaterialTheme.colorScheme.errorContainer, MaterialTheme.colorScheme.onErrorContainer) }
                reconnecting?.let {
                    Notice(it, MaterialTheme.colorScheme.secondaryContainer, MaterialTheme.colorScheme.onSecondaryContainer, progress = true)
                }

                Text("Computadores", style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(top = 8.dp))
                if (pcs.isEmpty()) {
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(vertical = 16.dp)) {
                        CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp)
                        Spacer(Modifier.width(16.dp))
                        Text("Procurando PCs com o TabDisplay aberto…", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                for (pc in pcs) PcCard(pc)

                TextButton(onClick = { manual = true }) { Text("Conectar pelo endereço IP") }
                Text(
                    "O PC precisa estar com o TabDisplay aberto e na mesma rede Wi‑Fi. " +
                        "Pelo cabo, ligue a depuração USB nas opções do desenvolvedor, " +
                        "ou, sem mexer nelas, compartilhe a internet do tablet pelo USB " +
                        "e toque no nome do PC assim que ele aparecer.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        }
        if (manual) ManualDialog { host -> manual = false; host?.let { connectManually(it) } }
    }

    @Composable
    private fun Notice(text: String, container: Color, content: Color, progress: Boolean = false) {
        Card(colors = CardDefaults.cardColors(containerColor = container, contentColor = content), modifier = Modifier.fillMaxWidth()) {
            Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                if (progress) {
                    CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp, color = content)
                    Spacer(Modifier.width(12.dp))
                }
                Text(text)
            }
        }
    }

    @Composable
    private fun PcCard(pc: Discovery.Pc) {
        val paired = pc.id?.let { prefs.getString("token_$it", null) } != null
        ElevatedCard(onClick = { autoConnect = true; connect(pc.host, pc.id, pc.name) }, modifier = Modifier.fillMaxWidth()) {
            ListItem(
                colors = ListItemDefaults.colors(containerColor = Color.Transparent),
                leadingContent = {
                    Surface(shape = CircleShape, color = MaterialTheme.colorScheme.primaryContainer, modifier = Modifier.size(40.dp)) {
                        Box(contentAlignment = Alignment.Center) {
                            Text(if (pc.usb) "USB" else "PC", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onPrimaryContainer)
                        }
                    }
                },
                headlineContent = { Text(if (pc.usb) "PC pelo cabo USB" else pc.name, fontWeight = FontWeight.Medium) },
                supportingContent = { Text(if (pc.usb) "Sem pareamento: o cabo basta" else "Wi‑Fi · ${pc.host}") },
                trailingContent = {
                    if (!pc.usb) {
                        Text(
                            if (paired) "Pareado" else "Novo",
                            style = MaterialTheme.typography.labelMedium,
                            color = if (paired) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                },
            )
        }
    }

    @Composable
    private fun ManualDialog(done: (String?) -> Unit) {
        var host by remember { mutableStateOf(prefs.getString("ip", "") ?: "") }
        AlertDialog(
            onDismissRequest = { done(null) },
            title = { Text("Conectar pelo IP") },
            text = {
                OutlinedTextField(
                    value = host,
                    onValueChange = { host = it.trim() },
                    label = { Text("Endereço IP do PC") },
                    placeholder = { Text("192.168.1.18") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                )
            },
            confirmButton = { TextButton(onClick = { done(host) }, enabled = host.isNotEmpty()) { Text("Conectar") } },
            dismissButton = { TextButton(onClick = { done(null) }) { Text("Cancelar") } },
        )
    }

    @Composable
    private fun PairDialog(request: PairRequest) {
        var code by remember(request) { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = {},
            title = { Text(if (request.wrong) "Código errado, tente de novo" else "Parear com ${request.pcName}") },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Text("Digite o código de 6 dígitos que apareceu no TabDisplay do PC. Só é pedido na primeira vez.")
                    OutlinedTextField(
                        value = code,
                        onValueChange = { code = it.filter(Char::isDigit).take(6) },
                        label = { Text("Código") },
                        singleLine = true,
                        textStyle = MaterialTheme.typography.headlineSmall,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
                    )
                }
            },
            confirmButton = {
                TextButton(onClick = { pairing = null; stream?.pair(code) }, enabled = code.length == 6) { Text("Parear") }
            },
            dismissButton = { TextButton(onClick = { disconnect() }) { Text("Cancelar") } },
        )
    }

    // ---- display screen ------------------------------------------------------------------------------

    @Composable
    private fun DisplayScreen(target: Screen.Display) {
        BackHandler { disconnect() }
        LaunchedEffect(Unit) {
            window.insetsController?.apply {
                hide(WindowInsets.Type.systemBars())
                systemBarsBehavior = WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            }
        }
        var menu by remember { mutableStateOf(false) }
        Box(Modifier.fillMaxSize().background(Color.Black)) {
            // Letterbox: the PC's aspect ratio in any tablet orientation.
            val ratio = video?.let { it.first.toFloat() / it.second }
            AndroidView(
                factory = { surface(it, target) },
                modifier = if (ratio != null) Modifier.align(Alignment.Center).aspectRatio(ratio) else Modifier.fillMaxSize(),
            )
            if (video == null) {
                Column(Modifier.align(Alignment.Center), horizontalAlignment = Alignment.CenterHorizontally) {
                    CircularProgressIndicator(color = Color.White)
                    Spacer(Modifier.height(16.dp))
                    Text("Conectando a ${target.name}…", color = Color.White)
                }
            }
            if (showStats) {
                stats?.let {
                    Text(
                        it,
                        color = Color.White,
                        style = MaterialTheme.typography.labelMedium,
                        modifier = Modifier.align(Alignment.TopStart).padding(12.dp)
                            .background(Color.Black.copy(alpha = 0.55f), RoundedCornerShape(6.dp)).padding(horizontal = 8.dp, vertical = 4.dp),
                    )
                }
            }
            // A small tab on the top edge opens the session menu without covering the desktop.
            Box(Modifier.align(Alignment.TopCenter)) {
                Surface(
                    onClick = { menu = true },
                    color = Color.Black.copy(alpha = 0.45f),
                    shape = RoundedCornerShape(bottomStart = 12.dp, bottomEnd = 12.dp),
                    modifier = Modifier.size(width = 72.dp, height = 20.dp),
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Box(Modifier.size(width = 28.dp, height = 3.dp).background(Color.White.copy(alpha = 0.8f), CircleShape))
                    }
                }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    DropdownMenuItem(
                        text = { Text(if (showStats) "Esconder estatísticas" else "Mostrar estatísticas") },
                        onClick = {
                            showStats = !showStats
                            prefs.edit().putBoolean("show_stats", showStats).apply()
                            menu = false
                        },
                    )
                    DropdownMenuItem(text = { Text("Desconectar") }, onClick = { menu = false; disconnect() })
                }
            }
        }
    }

    /** The video surface. Surface lifetime == session lifetime: leaving the app disconnects, coming back reconnects. */
    @SuppressLint("ClickableViewAccessibility")
    private fun surface(context: Context, target: Screen.Display): SurfaceView {
        val view = SurfaceView(context)
        var sentSize = 0 to 0
        view.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                val bounds = windowManager.currentWindowMetrics.bounds
                val (w, h) = decodableSize(bounds.width(), bounds.height())
                sentSize = w to h
                val hello = JSONObject()
                    .put("v", 2)
                    .put("device_id", deviceId)
                    .put("device_name", deviceName)
                    .put("token", target.pcId?.let { prefs.getString("token_$it", null) } ?: "")
                    .put("screen", JSONArray(listOf(bounds.width(), bounds.height())))
                    .put("decodable", JSONArray(listOf(w, h)))
                    .put("dpi", resources.displayMetrics.densityDpi)
                stream = Stream(target.host, holder.surface, hello, object : StreamEvents {
                    override fun onVideoSize(width: Int, height: Int) = runOnUiThread {
                        attempts = 0 // the session works: next drop retries quickly again
                        video = width to height
                    }
                    override fun onPairRequired(pcName: String, wrong: Boolean) = runOnUiThread { pairing = PairRequest(pcName, wrong) }
                    override fun onPaired(pcId: String, token: String) {
                        prefs.edit().putString("token_$pcId", token).putString("pc_at_${target.host}", pcId).apply()
                    }
                    override fun onStats(shownFps: Int, rttMs: Int, mbps: Double) = runOnUiThread {
                        stats = "$shownFps fps · $rttMs ms · ${"%.1f".format(mbps)} Mbps"
                    }
                    override fun onClose(reason: String) = runOnUiThread { endSession(reason) }
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
        view.setOnGenericMotionListener { v, e -> Input.encode(e, v.width, v.height)?.let { stream?.input(it) } != null }
        return view
    }

    // ---- session control -----------------------------------------------------------------------------

    private fun connect(host: String, pcId: String?, name: String) {
        pendingRetry?.let(handler::removeCallbacks)
        pendingRetry = null
        reconnecting = null
        video = null
        stats = null
        prefs.edit().putString("last_pc", pcId ?: if (host == "127.0.0.1") "usb" else null).apply()
        screen = Screen.Display(host, pcId, name)
    }

    private fun connectManually(host: String) {
        prefs.edit().putString("ip", host).apply()
        autoConnect = true
        connect(host, prefs.getString("pc_at_$host", null), host)
    }

    /** The user chose to leave: back to the list, and no automatic reconnection until they pick a PC. */
    private fun disconnect() {
        autoConnect = false
        stream?.close()
        endSession(null)
    }

    private fun endSession(reason: String?) {
        stream = null
        pairing = null
        video = null
        stats = null
        screen = Screen.Connect(reason)
    }

    private fun startDiscovery() {
        discovery?.stop()
        discovery = Discovery(this) { found ->
            runOnUiThread {
                pcs = found
                maybeReconnect(found)
            }
        }.also { it.start() }
    }

    private fun stopDiscovery() {
        discovery?.stop()
        discovery = null
        pendingRetry?.let(handler::removeCallbacks)
        pendingRetry = null
        reconnecting = null
    }

    /** Reconnects to the last PC once discovery sees it again, waiting longer after each failure. */
    private fun maybeReconnect(found: List<Discovery.Pc>) {
        if (!autoConnect || pendingRetry != null) return
        val last = prefs.getString("last_pc", null) ?: return
        val pc = found.firstOrNull { if (last == "usb") it.usb else it.id == last } ?: return
        val delay = RETRY_DELAYS_MS[attempts.coerceAtMost(RETRY_DELAYS_MS.lastIndex)]
        val name = if (pc.usb) "PC pelo cabo" else pc.name
        reconnecting = if (delay > 0) "Reconectando a $name em ${delay / 1000} s…" else "Conectando a $name…"
        pendingRetry = Runnable {
            pendingRetry = null
            attempts++
            connect(pc.host, pc.id, name)
        }.also { handler.postDelayed(it, delay) }
    }

    // ---- device facts --------------------------------------------------------------------------------

    private val deviceId: String
        get() = prefs.getString("device_id", null) ?: UUID.randomUUID().toString().also {
            prefs.edit().putString("device_id", it).apply()
        }

    private val deviceName: String
        get() = Settings.Global.getString(contentResolver, Settings.Global.DEVICE_NAME) ?: Build.MODEL

    /**
     * Largest size with the screen's aspect ratio that the H.264 decoder handles at 60 fps.
     * The Redmi Pad 2 screen is 2560x1600 but its decoder tops out at 2560x1440, so this gives 2304x1440
     * and the view scales it up.
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

/** Material 3 with the tablet's wallpaper colors (Android 12+), light or dark like the system. */
@Composable
private fun TabDisplayTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val context = LocalContext.current
    val colors = when {
        Build.VERSION.SDK_INT >= Build.VERSION_CODES.S -> if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        dark -> darkColorScheme()
        else -> lightColorScheme()
    }
    MaterialTheme(colorScheme = colors, content = content)
}
