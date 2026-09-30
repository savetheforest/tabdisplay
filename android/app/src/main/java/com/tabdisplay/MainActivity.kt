package com.tabdisplay

import android.annotation.SuppressLint
import android.content.Context
import android.content.ClipboardManager
import android.content.ClipData
import android.content.Intent
import android.media.MediaCodecInfo
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
import android.Manifest
import android.content.pm.PackageManager
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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
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
    private var showDiagnostics by mutableStateOf(false)
    private var muted by mutableStateOf(false)
    /** The PC's active quality preset, as last reported. */
    private var profile by mutableStateOf<String?>(null)
    /** Text received from the PC waits for an explicit copy or discard action. */
    private var incomingText by mutableStateOf<Pair<String, String>?>(null)

    private var stream: Stream? = null
    private var discovery: Discovery? = null
    /** Reconnect to the last PC when it shows up; off after the user disconnects on purpose. */
    private var autoConnect = true
    private var attempts = 0
    private var pendingRetry: Runnable? = null
    /** The video surface is gone (screen locked / another app in front) while a session is open. */
    private var backgrounded = false
    /** Monotonic owner for a connection attempt; callbacks from an older attempt are ignored. */
    private var sessionId = 0L
    /** Certificate candidates are memory-only until pairing/token authentication has succeeded. */
    private val candidatePins = mutableMapOf<String, String>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        showStats = prefs.getBoolean("show_stats", false)
        muted = prefs.getBoolean("muted", false)
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
        sessionId++
        stream?.close()
        stream = null
        SessionService.stop(this)
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
                Text("TabDisplay", style = MaterialTheme.typography.displaySmall, fontWeight = FontWeight.SemiBold, modifier = Modifier.semantics { heading() })
                Text(
                    stringResource(R.string.tagline),
                    style = MaterialTheme.typography.bodyLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(12.dp))

                message?.let { Notice(it, MaterialTheme.colorScheme.errorContainer, MaterialTheme.colorScheme.onErrorContainer) }
                reconnecting?.let {
                    Notice(it, MaterialTheme.colorScheme.secondaryContainer, MaterialTheme.colorScheme.onSecondaryContainer, progress = true)
                }

                Text(stringResource(R.string.computers), style = MaterialTheme.typography.titleMedium, modifier = Modifier.padding(top = 8.dp).semantics { heading() })
                Text(
                    stringResource(if (message != null) R.string.state_error else if (reconnecting != null) R.string.state_reconnecting else if (pcs.isEmpty()) R.string.state_searching else R.string.state_ready),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                )
                if (pcs.isEmpty()) {
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(vertical = 16.dp)) {
                        CircularProgressIndicator(Modifier.size(20.dp).clearAndSetSemantics {}, strokeWidth = 2.dp) // the text beside it says it all
                        Spacer(Modifier.width(16.dp))
                        Text(stringResource(R.string.searching), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                for (pc in pcs) PcCard(pc)

                TextButton(onClick = { manual = true }) { Text(stringResource(R.string.connect_by_ip)) }
                Text(
                    stringResource(R.string.connect_hint),
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
        // Live region: TalkBack reads a new error / reconnect notice without the user having to find it.
        Card(colors = CardDefaults.cardColors(containerColor = container, contentColor = content), modifier = Modifier.fillMaxWidth().semantics { liveRegion = LiveRegionMode.Polite }) {
            Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                if (progress) {
                    CircularProgressIndicator(Modifier.size(18.dp).clearAndSetSemantics {}, strokeWidth = 2.dp, color = content)
                    Spacer(Modifier.width(12.dp))
                }
                Text(text)
            }
        }
    }

    @Composable
    private fun PcCard(pc: Discovery.Pc) {
        val paired = pc.id?.let { prefs.getString("token_$it", null) } != null
        ElevatedCard(onClick = { autoConnect = true; connect(pc.host, pc.id ?: prefs.getString("pc_at_${pc.host}", null), pc.name) }, modifier = Modifier.fillMaxWidth()) {
            ListItem(
                colors = ListItemDefaults.colors(containerColor = Color.Transparent),
                leadingContent = {
                    Surface(shape = CircleShape, color = MaterialTheme.colorScheme.primaryContainer, modifier = Modifier.size(40.dp)) {
                        Box(contentAlignment = Alignment.Center) {
                            Text(stringResource(R.string.badge_pc), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onPrimaryContainer)
                        }
                    }
                },
                headlineContent = { Text(if (pc.local) stringResource(R.string.pc_via_local) else pc.name, fontWeight = FontWeight.Medium) },
                supportingContent = { Text(if (pc.local) stringResource(R.string.local_endpoint) else stringResource(R.string.wifi_host, pc.host)) },
                trailingContent = {
                    if (!pc.local) {
                        Text(
                            stringResource(if (paired) R.string.paired else R.string.new_pc),
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
            title = { Text(stringResource(R.string.connect_by_ip_title)) },
            text = {
                OutlinedTextField(
                    value = host,
                    onValueChange = { host = it.trim() },
                    label = { Text(stringResource(R.string.ip_label)) },
                    placeholder = { Text("192.168.1.18") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                )
            },
            confirmButton = { TextButton(onClick = { done(host) }, enabled = host.isNotEmpty()) { Text(stringResource(R.string.connect)) } },
            dismissButton = { TextButton(onClick = { done(null) }) { Text(stringResource(R.string.cancel)) } },
        )
    }

    @Composable
    private fun PairDialog(request: PairRequest) {
        var code by remember(request) { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = {},
            title = { Text(if (request.wrong) stringResource(R.string.pair_wrong) else stringResource(R.string.pair_title, request.pcName)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Text(stringResource(R.string.pair_body))
                    OutlinedTextField(
                        value = code,
                        onValueChange = { code = it.filter(Char::isDigit).take(6) },
                        label = { Text(stringResource(R.string.code_label)) },
                        singleLine = true,
                        textStyle = MaterialTheme.typography.headlineSmall,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword),
                    )
                }
            },
            confirmButton = {
                TextButton(onClick = { pairing = null; stream?.pair(code) }, enabled = code.length == 6) { Text(stringResource(R.string.pair)) }
            },
            dismissButton = { TextButton(onClick = { disconnect() }) { Text(stringResource(R.string.cancel)) } },
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
        var showKeyboard by remember { mutableStateOf(false) }
        val screenDescription = stringResource(R.string.a11y_screen)
        val menuDescription = stringResource(R.string.a11y_menu)
        Box(Modifier.fillMaxSize().background(Color.Black)) {
            // Letterbox: the PC's aspect ratio in any tablet orientation.
            val ratio = video?.let { it.first.toFloat() / it.second }
            AndroidView(
                factory = { surface(it, target) },
                modifier = (if (ratio != null) Modifier.align(Alignment.Center).aspectRatio(ratio) else Modifier.fillMaxSize())
                    .semantics { contentDescription = screenDescription },
            )
            if (video == null) {
                Column(Modifier.align(Alignment.Center), horizontalAlignment = Alignment.CenterHorizontally) {
                    CircularProgressIndicator(color = Color.White, modifier = Modifier.clearAndSetSemantics {})
                    Spacer(Modifier.height(16.dp))
                    Text(stringResource(R.string.connecting_to, target.name), color = Color.White)
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
                    modifier = Modifier.size(width = 96.dp, height = 48.dp).semantics { contentDescription = menuDescription; role = Role.Button },
                ) {
                    Box(contentAlignment = Alignment.Center) {
                        Box(Modifier.size(width = 36.dp, height = 4.dp).background(Color.White.copy(alpha = 0.8f), CircleShape))
                    }
                }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    DropdownMenuItem(
                        text = { Text(stringResource(if (showStats) R.string.hide_stats else R.string.show_stats)) },
                        onClick = {
                            showStats = !showStats
                            prefs.edit().putBoolean("show_stats", showStats).apply()
                            menu = false
                        },
                    )
                    for ((id, label) in listOf("performance" to R.string.profile_performance, "balanced" to R.string.profile_balanced, "quality" to R.string.profile_quality, "auto" to R.string.profile_auto)) {
                        DropdownMenuItem(
                            text = { Text(stringResource(label)) },
                            trailingIcon = { if (profile == id) Text("✓") },
                            onClick = { menu = false; profile = id; stream?.setProfile(id) },
                        )
                    }
                    if (profile == "custom") {
                        DropdownMenuItem(text = { Text(stringResource(R.string.profile_custom)) }, trailingIcon = { Text("✓") }, enabled = false, onClick = {})
                    }
                    DropdownMenuItem(text = { Text(stringResource(R.string.diagnostics)) }, onClick = { menu = false; showDiagnostics = true })
                    DropdownMenuItem(
                        text = { Text(stringResource(R.string.send_copied_text)) },
                        onClick = {
                            menu = false
                            val clipboard = getSystemService(CLIPBOARD_SERVICE) as ClipboardManager
                            val text = clipboard.primaryClip?.getItemAt(0)?.coerceToText(this@MainActivity)?.toString()
                            if (text == null) {
                                android.widget.Toast.makeText(this@MainActivity, R.string.clipboard_empty, android.widget.Toast.LENGTH_SHORT).show()
                            } else {
                                runCatching { stream?.sendText(text) }.onSuccess {
                                    android.widget.Toast.makeText(this@MainActivity, R.string.text_sent, android.widget.Toast.LENGTH_SHORT).show()
                                }.onFailure {
                                    android.widget.Toast.makeText(this@MainActivity, R.string.text_too_large, android.widget.Toast.LENGTH_SHORT).show()
                                }
                            }
                        },
                    )
                    DropdownMenuItem(text = { Text(stringResource(R.string.keyboard)) }, onClick = { menu = false; showKeyboard = true })
                    DropdownMenuItem(
                        text = { Text(stringResource(if (muted) R.string.unmute_audio else R.string.mute_audio)) },
                        onClick = {
                            muted = !muted
                            stream?.muted = muted
                            prefs.edit().putBoolean("muted", muted).apply()
                            menu = false
                        },
                    )
                    DropdownMenuItem(text = { Text(stringResource(R.string.disconnect)) }, onClick = { menu = false; disconnect() })
                }
            }
        }
        if (showDiagnostics) DiagnosticsDialog(target) { showDiagnostics = false }
        incomingText?.let { (sender, text) ->
            TextReceivedDialog(sender, text) { incomingText = null }
        }
        if (showKeyboard) KeyboardDialog { showKeyboard = false }
    }

    @Composable
    private fun TextReceivedDialog(sender: String, text: String, close: () -> Unit) {
        AlertDialog(
            onDismissRequest = close,
            title = { Text(stringResource(R.string.text_received_title)) },
            text = { Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.text_received_from, sender))
                Text(text, style = MaterialTheme.typography.bodyMedium)
                Text(stringResource(R.string.text_received_hint), style = MaterialTheme.typography.bodySmall)
            } },
            confirmButton = { TextButton(onClick = {
                val clipboard = getSystemService(CLIPBOARD_SERVICE) as ClipboardManager
                clipboard.setPrimaryClip(ClipData.newPlainText("TabDisplay", text))
                close()
            }) { Text(stringResource(R.string.copy_text)) } },
            dismissButton = { TextButton(onClick = close) { Text(stringResource(R.string.discard_text)) } },
        )
    }

    @Composable
    private fun KeyboardDialog(close: () -> Unit) {
        var text by remember { mutableStateOf("") }
        var modifiers by remember { mutableStateOf(0) }
        val toggle: (Int) -> Unit = { bit -> modifiers = modifiers xor bit }
        AlertDialog(
            onDismissRequest = close,
            title = { Text(stringResource(R.string.keyboard)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(stringResource(R.string.keyboard_hint), style = MaterialTheme.typography.bodySmall)
                    OutlinedTextField(
                        value = text,
                        onValueChange = { text = it },
                        label = { Text(stringResource(R.string.keyboard_text_label)) },
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Text),
                        minLines = 2,
                    )
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        for ((label, bit) in listOf("Ctrl" to Protocol.MOD_CTRL, "Shift" to Protocol.MOD_SHIFT, "Alt" to Protocol.MOD_ALT)) {
                            TextButton(onClick = { toggle(bit) }) { Text(if (modifiers and bit != 0) "✓ $label" else label) }
                        }
                    }
                    Row(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        for (key in listOf("Enter", "Backspace", "Tab", "Escape", "ArrowLeft", "ArrowRight")) {
                            TextButton(onClick = { stream?.sendKey(key, modifiers) }) { Text(key) }
                        }
                    }
                }
            },
            confirmButton = { TextButton(onClick = {
                runCatching { stream?.sendKeyText(text) }
                text = ""
            }) { Text(stringResource(R.string.send_text)) } },
            dismissButton = { TextButton(onClick = close) { Text(stringResource(R.string.cancel)) } },
        )
    }

    @Composable
    private fun DiagnosticsDialog(target: Screen.Display, close: () -> Unit) {
        AlertDialog(
            onDismissRequest = close,
            title = { Text(stringResource(R.string.diagnostics)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(stringResource(R.string.diagnostics_text, target.name, target.host, profile ?: "—", stats ?: "—"))
                    Text(stringResource(R.string.diagnostics_limit), style = MaterialTheme.typography.bodySmall)
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    val intent = Intent(Intent.ACTION_SEND).apply {
                        type = "text/plain"
                        putExtra(Intent.EXTRA_TEXT, Crumbs.exportText())
                    }
                    startActivity(Intent.createChooser(intent, getString(R.string.export_diagnostics)))
                    close()
                }) { Text(stringResource(R.string.export_diagnostics)) }
            },
            dismissButton = { TextButton(onClick = close) { Text(stringResource(R.string.cancel)) } },
        )
    }

    /** The video surface. The session outlives it: on lock / app switch only the decoder stops (see [Stream.detach]). */
    @SuppressLint("ClickableViewAccessibility")
    private fun surface(context: Context, target: Screen.Display): SurfaceView {
        val view = SurfaceView(context)
        var sentSize = 0 to 0
        view.holder.addCallback(object : SurfaceHolder.Callback {
            override fun surfaceCreated(holder: SurfaceHolder) {
                val bounds = windowManager.currentWindowMetrics.bounds
                val candidates = decoderCapabilities(bounds.width(), bounds.height())
                val selected = VideoNegotiation.select(candidates) ?: run {
                    endSession(getString(R.string.no_decoder))
                    return
                }
                val initial = VideoNegotiation.modeFor(selected, bounds.width(), bounds.height(), 60)
                    ?: selected.modes.maxBy { it.width.toLong() * it.height }
                val (w, h) = initial.width to initial.height
                sentSize = w to h
                backgrounded = false
                window.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                // Coming back from lock / another app: the session is still alive, just give it the new surface.
                stream?.let {
                    SessionService.resume(this@MainActivity, target.name)
                    it.attach(holder.surface, w, h)
                    return
                }
                if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
                    requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), NOTIFICATION_REQUEST)
                }
                val owner = sessionId
                val service = runCatching { SessionService.start(this@MainActivity, target.name) }
                if (service.isFailure) {
                    val error = service.exceptionOrNull()!!
                    endSession(getString(R.string.error_generic, error.message ?: error.javaClass.simpleName))
                    return
                }
                val hello = JSONObject()
                    .put("v", Protocol.VERSION)
                    .put("device_id", deviceId)
                    .put("device_name", deviceName)
                    .put("token", target.pcId?.let { prefs.getString("token_$it", null) } ?: "")
                    .put("screen", JSONArray(listOf(bounds.width(), bounds.height())))
                    .put("decodable", JSONArray(listOf(w, h)))
                    .put("video_modes", JSONArray().apply {
                        selected.modes.forEach { mode ->
                            put(JSONObject().put("width", mode.width).put("height", mode.height).put("fps", mode.fps))
                        }
                    })
                    .put("audio_timestamps", true)
                    .put("text_transfer", true)
                    .put("keyboard", true)
                    .put("dpi", resources.displayMetrics.densityDpi)
                stream = Stream(applicationContext, target.host, holder.surface, hello, candidates.map { it.name }, object : StreamEvents {
                    override fun onVideoSize(width: Int, height: Int) = runOnUiThread {
                        if (owner != sessionId) return@runOnUiThread
                        target.pcId?.let { pcId ->
                            candidatePins.remove(target.host)?.let { fingerprint ->
                                if (prefs.getString("pin_$pcId", null) == null) prefs.edit().putString("pin_$pcId", fingerprint).apply()
                            }
                        }
                        attempts = 0 // the session works: next drop retries quickly again
                        video = width to height
                    }
                    override fun onPairRequired(pcName: String, wrong: Boolean) = runOnUiThread {
                        if (owner == sessionId) pairing = PairRequest(pcName, wrong)
                    }
                    override fun onPaired(pcId: String, token: String) {
                        if (owner == sessionId) {
                            val candidate = candidatePins.remove(target.host)
                            prefs.edit().putString("last_pc", pcId).putString("token_$pcId", token).putString("pc_at_${target.host}", pcId).apply {
                                if (candidate != null) putString("pin_$pcId", candidate)
                            }.apply()
                        }
                    }
                    // Trust on first use: pin the PC's certificate; a different one later means another machine answered.
                    override fun onServerCertificate(fingerprint: String): Boolean {
                        val key = "pin_" + (target.pcId ?: target.host)
                        val pinned = prefs.getString(key, null)
                        if (pinned != null && pinned != fingerprint) return false
                        candidatePins[target.host] = fingerprint
                        return true
                    }
                    override fun onProfile(profile: String) = runOnUiThread { if (owner == sessionId) this@MainActivity.profile = profile }
                    override fun onStats(shownFps: Int, rttMs: Int, mbps: Double) = runOnUiThread {
                        if (owner == sessionId) stats = getString(R.string.stats_format, shownFps, rttMs, "%.1f".format(mbps))
                    }
                    override fun onTextReceived(sender: String, text: String) = runOnUiThread {
                        if (owner == sessionId) incomingText = sender to text
                    }
                    override fun onClose(reason: String) = runOnUiThread { if (owner == sessionId) endSession(reason) }
                }).also { it.muted = muted; it.start() }
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
                backgrounded = true
                window.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
                SessionService.pause(this@MainActivity)
                stream?.detach()
            }
        })
        // Every finger and the pen (including hovering) go to the PC as one INPUT frame per event.
        view.setOnTouchListener { v, e ->
            Input.encode(e, v.width, v.height)?.let { stream?.input(it) }
            true
        }
        view.setOnGenericMotionListener { v, e ->
            Input.scroll(e, v.width, v.height)?.let { stream?.scroll(it) } != null ||
                Input.encode(e, v.width, v.height)?.let { stream?.input(it) } != null
        }
        return view
    }

    // ---- session control -----------------------------------------------------------------------------

    private fun connect(host: String, pcId: String?, name: String) {
        sessionId++
        stream?.close()
        stream = null
        candidatePins.clear()
        pendingRetry?.let(handler::removeCallbacks)
        pendingRetry = null
        reconnecting = null
        video = null
        stats = null
        prefs.edit().putString("last_pc", pcId ?: if (host == "127.0.0.1") "local" else null).apply()
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
        sessionId++
        stream?.close()
        stream = null
        SessionService.stop(this)
        // Say why the list is back when the session died while the screen was locked / the app hidden.
        val shown = if (reason != null && backgrounded) getString(R.string.dropped_in_background, reason) else reason
        backgrounded = false
        pairing = null
        video = null
        stats = null
        profile = null
        if (!SessionRetry.allowsAutomaticRetry(reason)) {
            autoConnect = false
        }
        screen = Screen.Connect(shown)
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
        val pc = found.firstOrNull { if (last == "local") it.local else it.id == last } ?: return
        val delay = SessionRetry.delayMs(attempts)
        val name = if (pc.local) getString(R.string.pc_via_local) else pc.name
        reconnecting = if (delay > 0) getString(R.string.reconnecting_in, name, delay / 1000) else getString(R.string.connecting_to, name)
        pendingRetry = Runnable {
            pendingRetry = null
            attempts++
        connect(pc.host, pc.id ?: prefs.getString("pc_at_${pc.host}", null), name)
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
    private fun decoderCapabilities(width: Int, height: Int): List<DecoderCapability> =
        MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.mapNotNull { info ->
            if (info.isEncoder || MediaFormat.MIMETYPE_VIDEO_AVC !in info.supportedTypes || info.name.endsWith(".secure")) return@mapNotNull null
            val capabilities = runCatching { info.getCapabilitiesForType(MediaFormat.MIMETYPE_VIDEO_AVC) }.getOrNull() ?: return@mapNotNull null
            if (capabilities.isFeatureRequired(MediaCodecInfo.CodecCapabilities.FEATURE_TunneledPlayback)) return@mapNotNull null
            val video = capabilities.videoCapabilities ?: return@mapNotNull null
            val modes = VideoNegotiation.standardFps.mapNotNull { fps ->
                (100 downTo 25).asSequence().mapNotNull { percent ->
                    val w = (width * percent / 100) and 15.inv()
                    val h = (height * percent / 100) and 15.inv()
                    if (w >= Protocol.MIN_DIMENSION && h >= Protocol.MIN_DIMENSION && video.areSizeAndRateSupported(w, h, fps.toDouble())) {
                        VideoMode(w, h, fps)
                    } else null
                }.firstOrNull()
            }
            DecoderCapability(info.name, info.canonicalName, info.isHardwareAccelerated, info.isSoftwareOnly, modes)
        }

    private fun decodableSize(width: Int, height: Int): Pair<Int, Int> {
        val selected = VideoNegotiation.select(decoderCapabilities(width, height)) ?: return 1280 to 720
        val mode = VideoNegotiation.modeFor(selected, width, height, 60) ?: return 1280 to 720
        return mode.width to mode.height
    }

    private companion object {
        const val NOTIFICATION_REQUEST = 1001
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
