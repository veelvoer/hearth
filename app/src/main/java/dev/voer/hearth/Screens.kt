package dev.voer.hearth

import android.Manifest
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.StrokeJoin
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import dev.voer.hearth.Store.error
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle as JTextStyle
import java.util.Locale

data class PendingOpen(val host: String, val session: String, val title: String, val cwd: String, val voice: Boolean = false)

object Ui {
    var usage by mutableStateOf<Usage?>(null)
    var profile by mutableStateOf<Profile?>(null)
    var signedIn by mutableStateOf(false)
    var tourRev by mutableStateOf(0)
    var newRun by mutableStateOf("vps")   // which computer a brand-new chat should start on
    var error by mutableStateOf<String?>(null)
    var busy by mutableStateOf(false)
    var history by mutableStateOf<List<Sample>>(emptyList())
    var rev by mutableIntStateOf(0)
    var chatOpen by mutableStateOf(false)
    var tab by mutableIntStateOf(0)
    var machinesRev by mutableIntStateOf(0)
    var pendingOpen by mutableStateOf<PendingOpen?>(null)

    fun load(c: Context) {
        signedIn = Store.signedIn(c); usage = Store.usage(c); profile = Store.profile(c)
        history = Store.history(c); error = with(Store) { c.error }; rev++
    }

    suspend fun refresh(c: Context, ask: Boolean = false) {
        if (busy || !Store.signedIn(c)) return
        busy = true
        Repo.refresh(c, ask)
        load(c)
        busy = false
    }
}

@Composable
fun tick(): State<Long> = produceState(System.currentTimeMillis()) {
    while (true) { delay(30_000); value = System.currentTimeMillis() }
}

@Composable
fun Screen(content: @Composable ColumnScope.() -> Unit) =
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 16.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp), content = content)

@Composable
fun Header(title: String, sub: String? = null, action: (@Composable () -> Unit)? = null) =
    Row(Modifier.fillMaxWidth().padding(bottom = 4.dp), verticalAlignment = Alignment.Bottom) {
        Column(Modifier.weight(1f)) {
            Txt(title, T.title)
            if (sub != null) Txt(sub, T.small, muted = true)
        }
        action?.invoke()
    }

private val hm = DateTimeFormatter.ofPattern("HH:mm")
private fun at(ms: Long, week: Boolean): String {
    val z = Instant.ofEpochMilli(ms).atZone(ZoneId.systemDefault())
    return if (week) z.dayOfWeek.getDisplayName(JTextStyle.SHORT, Locale.getDefault()) + " " + hm.format(z) else hm.format(z)
}

private fun ago(t: Long, now: Long): String {
    val m = (now - t) / 60_000
    return if (m < 1) "just now" else "${dur(now - t)} ago"
}

@Composable
fun LimitCard(name: String, window: String, l: Limit?, ms: Long, now: Long) {
    Card {
        Row(Modifier.fillMaxWidth()) { Label(name); Spacer(Modifier.weight(1f)); Label(window) }
        if (l == null) { Txt("No data", T.body, muted = true, modifier = Modifier.padding(top = 12.dp)); return@Card }
        val p = l.at(now)
        val pc = pace(l, ms, now)
        Row(verticalAlignment = Alignment.Bottom, modifier = Modifier.padding(top = 4.dp, bottom = 6.dp)) {
            Txt(p.toInt().toString(), T.big)
            Txt("%", T.title, muted = true, modifier = Modifier.padding(bottom = 12.dp, start = 2.dp))
        }
        Meter(p.toFloat(), pc?.elapsed?.toFloat())
        Spacer(Modifier.height(6.dp))
        val r = l.resetsAt
        Row(Modifier.fillMaxWidth()) {
            if (r != null && r > now) {
                Txt("Resets in ${dur(r - now)}", T.small); Spacer(Modifier.weight(1f)); Txt(at(r, ms == WEEK_MS), T.small, muted = true)
            } else Txt("Fresh window — starts on your next message", T.small, muted = true)
        }
        paceWord(l, pc)?.let { w ->
            val detail = when {
                pc?.hitsAt != null -> "At this rate you hit the limit in ${dur(pc.hitsAt - now)}."
                pc != null && pc.projected > 0 -> "Heading for about ${pc.projected.coerceAtMost(100.0).toInt()}% by reset."
                else -> ""
            }
            Spacer(Modifier.height(10.dp))
            Txt("$w. $detail".trim(), T.small, muted = true)
        }
    }
}

@Composable
fun NowScreen() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    val now by tick()
    val u = Ui.usage
    Screen {
        Header("Hearth", Ui.profile?.let { listOf(it.plan, it.email).filter { s -> s.isNotEmpty() }.joinToString(" · ") }) {
            TextAction(if (Ui.busy) "Updating…" else "Refresh") { scope.launch { Ui.refresh(c, ask = true) } }
        }
        Ui.error?.let {
            Card { Txt(it, T.small, color = MaterialTheme.colorScheme.primary) }
        }
        LimitCard("Session", "5-hour window", u?.session, SESSION_MS, now)
        LimitCard("Week", "7-day window", u?.week, WEEK_MS, now)
        if (u?.opus != null || u?.sonnet != null) Card {
            Label("Weekly by model")
            listOf("Opus" to u.opus, "Sonnet" to u.sonnet).forEach { (n, l) ->
                if (l != null) {
                    Spacer(Modifier.height(12.dp))
                    Row { Txt(n, T.body); Spacer(Modifier.weight(1f)); Txt("${l.at(now).toInt()}%", T.body, muted = true) }
                    Meter(l.at(now).toFloat(), height = 5.dp)
                }
            }
        }
        u?.extra?.let { x ->
            Card {
                Label("Extra usage")
                Spacer(Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.Bottom) {
                    Txt("$" + "%.2f".format(x.used), T.title); Txt("  of $" + "%.2f".format(x.limit), T.small, muted = true)
                }
                Meter(x.pct.toFloat(), height = 5.dp)
            }
        }
        if (u != null) Txt("Updated ${ago(u.at, now)}. Same limits as /usage in Claude Code.", T.small, muted = true)
    }
}

@Composable
fun Chart(data: List<Sample>, from: Long, to: Long, modifier: Modifier = Modifier) {
    val cs = MaterialTheme.colorScheme
    val grid = cs.outline; val sCol = cs.primary; val wCol = cs.onSurface; val txt = cs.onSurfaceVariant
    val dens = LocalDensity.current
    Canvas(modifier.fillMaxWidth().height(190.dp)) {
        val left = 30.dp.toPx(); val top = 8.dp.toPx(); val bottom = size.height - 18.dp.toPx(); val right = size.width
        fun x(t: Long) = left + (right - left) * (t - from).toFloat() / (to - from).coerceAtLeast(1)
        fun y(v: Double) = bottom - (bottom - top) * (v / 100).toFloat()
        val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply {
            color = txt.hashCode().let { android.graphics.Color.argb((txt.alpha * 255).toInt(), (txt.red * 255).toInt(), (txt.green * 255).toInt(), (txt.blue * 255).toInt()) }
            textSize = 10.sp.toPx()
        }
        listOf(0, 50, 100).forEach { v ->
            drawLine(grid, Offset(left, y(v.toDouble())), Offset(right, y(v.toDouble())), 1.dp.toPx())
            drawContext.canvas.nativeCanvas.drawText("$v", 0f, y(v.toDouble()) + 4.dp.toPx(), paint)
        }
        val span = to - from
        val startLbl = if (span <= 36 * 3600_000L) "24h ago" else "${span / 86_400_000}d ago"
        drawContext.canvas.nativeCanvas.drawText(startLbl, left, size.height - 2.dp.toPx(), paint)
        paint.textAlign = android.graphics.Paint.Align.RIGHT
        drawContext.canvas.nativeCanvas.drawText("now", right, size.height - 2.dp.toPx(), paint)
        fun series(sel: (Sample) -> Double, col: androidx.compose.ui.graphics.Color, w: Float) {
            val path = Path(); var prev: Sample? = null
            data.forEach { s ->
                val v = sel(s)
                if (prev == null || v < sel(prev!!) - 5) path.moveTo(x(s.t), y(v)) else path.lineTo(x(s.t), y(v))
                prev = s
            }
            drawPath(path, col, style = Stroke(w, cap = StrokeCap.Round, join = StrokeJoin.Round))
            data.lastOrNull()?.let { drawCircle(col, 3.5.dp.toPx(), Offset(x(it.t), y(sel(it)))) }
        }
        series({ it.w }, wCol, 1.5.dp.toPx())
        series({ it.s }, sCol, 2.dp.toPx())
    }
}

@Composable
fun HistoryScreen() {
    var range by remember { mutableIntStateOf(0) }
    val now by tick()
    val spans = listOf(24L * 3600_000, 7L * 86_400_000, 30L * 86_400_000)
    val from = now - spans[range]
    val data = Ui.history.filter { it.t >= from }
    Screen {
        Header("History", "Sampled every time the app or a widget refreshes")
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("24h", "7d", "30d").forEachIndexed { i, s -> Chip(s, range == i) { range = i } }
        }
        Card {
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp), modifier = Modifier.padding(bottom = 10.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(8.dp).clip(RoundedCornerShape(50)).background(MaterialTheme.colorScheme.primary)); Spacer(Modifier.width(6.dp)); Txt("Session", T.small, muted = true)
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(8.dp).clip(RoundedCornerShape(50)).background(MaterialTheme.colorScheme.onSurface)); Spacer(Modifier.width(6.dp)); Txt("Week", T.small, muted = true)
                }
            }
            if (data.size < 2) Txt("Not enough samples yet. Come back after a few refreshes.", T.small, muted = true,
                modifier = Modifier.padding(vertical = 40.dp))
            else Chart(data, from, now)
        }
        if (data.isNotEmpty()) {
            var hits = 0
            data.zipWithNext { p, n -> if (p.s < 90 && n.s >= 90) hits++ }
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                listOf("Peak session" to "${data.maxOf { it.s }.toInt()}%", "Peak week" to "${data.maxOf { it.w }.toInt()}%", "Hit 90%+" to "$hits×").forEach { (k, v) ->
                    Card(Modifier.weight(1f)) { Label(k); Spacer(Modifier.height(6.dp)); Txt(v, T.h2) }
                }
            }
            val zone = ZoneId.systemDefault()
            val days = if (range == 2) 14 else 7
            val byDay = Ui.history.groupBy { Instant.ofEpochMilli(it.t).atZone(zone).toLocalDate() }
            val today = Instant.ofEpochMilli(now).atZone(zone).toLocalDate()
            Card {
                Label("Daily session peak")
                Spacer(Modifier.height(12.dp))
                Row(Modifier.fillMaxWidth().height(110.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.Bottom) {
                    for (i in days - 1 downTo 0) {
                        val d = today.minusDays(i.toLong())
                        val peak = byDay[d]?.maxOf { it.s } ?: 0.0
                        Column(Modifier.weight(1f).fillMaxHeight(), verticalArrangement = Arrangement.Bottom, horizontalAlignment = Alignment.CenterHorizontally) {
                            Box(Modifier.fillMaxWidth().weight(1f, fill = true), contentAlignment = Alignment.BottomCenter) {
                                Box(Modifier.fillMaxWidth().fillMaxHeight((peak / 100).toFloat().coerceIn(0.02f, 1f))
                                    .clip(RoundedCornerShape(topStart = 4.dp, topEnd = 4.dp))
                                    .background(if (peak == 0.0) MaterialTheme.colorScheme.outline else MaterialTheme.colorScheme.primary))
                            }
                            Txt(d.dayOfWeek.getDisplayName(JTextStyle.NARROW, Locale.getDefault()), T.label, muted = true, modifier = Modifier.padding(top = 4.dp))
                        }
                    }
                }
            }
        }
    }
}

@Composable
fun Preview(cfg: WCfg, shape: Shape, u: Usage?) {
    val c = LocalContext.current
    val d = LocalDensity.current.density
    val bmp = remember(cfg, shape, u) { WidgetUpdater.bitmap(c, cfg, shape, u ?: sampleUsage(), shape.w, shape.h, d) }
    Image(bmp.asImageBitmap(), "Widget preview", Modifier.fillMaxWidth(when (shape) { Shape.TINY -> .3f; Shape.SMALL -> .5f; else -> 1f }).aspectRatio(shape.w / shape.h), contentScale = ContentScale.FillBounds)
}

fun sampleUsage(): Usage {
    val n = System.currentTimeMillis()
    return Usage(Limit(42.0, n + 2 * 3600_000L + 14 * 60_000L), Limit(18.0, n + 3 * 86_400_000L), null, null, null, n)
}

@Composable
fun WidgetsScreen() {
    val c = LocalContext.current
    Screen {
        Header("Widgets", "Three shapes. Each can be styled to match Nothing OS, Claude, or your system colors.")
        Shape.entries.forEach { s ->
            val cfg = remember(Ui.rev) { Store.widget(c, 0) }
            Card {
                Label(s.label)
                Spacer(Modifier.height(12.dp))
                Preview(cfg, s, Ui.usage)
                Spacer(Modifier.height(12.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(20.dp)) {
                    TextAction("Customize") { c.startActivity(Intent(c, WidgetConfigActivity::class.java).putExtra("shape", s.name)) }
                    TextAction("Add to home screen") { pin(c, s) }
                }
            }
        }
        Txt("Customizing here sets the default for new widgets. After a widget is on your home screen, long-press it and tap Reconfigure to style that one alone.", T.small, muted = true)
    }
}

private fun pin(c: Context, s: Shape) {
    val m = android.appwidget.AppWidgetManager.getInstance(c)
    val cls = when (s) { Shape.TINY -> TinyWidget::class.java; Shape.SMALL -> SmallWidget::class.java; Shape.WIDE -> WideWidget::class.java; Shape.STRIP -> StripWidget::class.java }
    if (m.isRequestPinAppWidgetSupported) m.requestPinAppWidget(android.content.ComponentName(c, cls), null, null)
    else android.widget.Toast.makeText(c, "Long-press your home screen → Widgets → Hearth", android.widget.Toast.LENGTH_LONG).show()
}

@Composable
fun Row2(label: String, sub: String? = null, trailing: @Composable () -> Unit) =
    Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) { Txt(label, T.body); if (sub != null) Txt(sub, T.small, muted = true) }
        trailing()
    }

@Composable
fun UsageScreen() {
    var view by remember { mutableIntStateOf(0) }
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.padding(start = 20.dp, end = 20.dp, top = 14.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Chip("Limits", view == 0) { view = 0 }; Chip("History", view == 1) { view = 1 }
        }
        Box(Modifier.weight(1f).tourTarget("usage")) { if (view == 0) NowScreen() else HistoryScreen() }
    }
}

@Composable
fun SettingsScreen() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var support by remember { mutableStateOf(false) }
    if (support) { SupportScreen { support = false }; return }
    var thr by remember { mutableStateOf(Store.thresholds(c)) }
    var interval by remember { mutableIntStateOf(Store.interval(c)) }
    var nreset by remember { mutableStateOf(Store.notifyReset(c)) }
    var live by remember { mutableStateOf(Store.live(c)) }
    val perm = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) {}
    fun ask() { if (Build.VERSION.SDK_INT >= 33) perm.launch(Manifest.permission.POST_NOTIFICATIONS) }
    var sub by remember { mutableStateOf<String?>(null) }
    if (sub == "widgets") {
        BackHandler { sub = null }
        Column(Modifier.fillMaxSize()) {
            Row(Modifier.padding(start = 20.dp, top = 16.dp)) { TextAction("‹ Settings") { sub = null } }
            Box(Modifier.weight(1f)) { WidgetsScreen() }
        }
        return
    }
    Screen {
        Header("Settings")
        ComputersCard()
        UpdatesCard()
        Card { Row(verticalAlignment = Alignment.CenterVertically) { Column(Modifier.weight(1f)) { Label("Help"); Txt("New here? Take the tour or run the tutorial again.", T.small, muted = true) }; Column(horizontalAlignment = Alignment.End) { TextAction("Take the tour") { Tour.start() }; Spacer(Modifier.height(6.dp)); TextAction("Show tutorial") { Store.setOnboarded(c, false); Ui.tourRev++ } } } }
        Card {
            Label("Notifications and calls")
            Spacer(Modifier.height(8.dp))
            var callMe by remember { mutableStateOf(Store.callMe(c)) }
            Row2("Stay connected to Claude", "Messages when a session finishes or needs you, even when the app is closed. Calls only when you ask for one in a chat.") {
                Switch(callMe, {
                    callMe = it; Store.setCallMe(c, it); ask()
                    if (it) CallService.sync(c) else CallService.turnOff(c)
                })
            }
            Spacer(Modifier.height(6.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(20.dp)) {
                TextAction("Allow background activity") {
                    runCatching { c.startActivity(Intent(android.provider.Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:" + c.packageName))) }
                }
                TextAction("Test call") {
                    Calls.ring(c, Store.machines(c).firstOrNull() ?: Machine("test", "", 47601, ""),
                        CallEvent(System.currentTimeMillis(), "permission", "", "Test session", "", "Bash: npm test", 0, "", "test"))
                }
                if (Build.VERSION.SDK_INT >= 34) TextAction("Allow full-screen calls") {
                    c.startActivity(Intent(android.provider.Settings.ACTION_MANAGE_APP_USE_FULL_SCREEN_INTENT, Uri.parse("package:" + c.packageName)))
                }
            }
            Spacer(Modifier.height(6.dp))
            Txt("Tip: allow background activity so Android never puts the connection to sleep. Calls need the full-screen permission on Android 14 and newer.", T.small, muted = true)
        }
        Card {
            Label("Appearance")
            Spacer(Modifier.height(10.dp))
            val sys = ThemePrefs.system == true
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Chip("Claude", !sys) { ThemePrefs.system = false; Store.setThemeSystem(c, false) }
                if (Build.VERSION.SDK_INT >= 31) Chip("System colors", sys) { ThemePrefs.system = true; Store.setThemeSystem(c, true) }
            }
            Spacer(Modifier.height(8.dp))
            Txt(if (Build.VERSION.SDK_INT >= 31) "System colors uses your phone's wallpaper colors for the whole app. Light and dark always follow your phone." else "Light and dark follow your phone.", T.small, muted = true)
        }
        Card(Modifier.clickable { sub = "widgets" }) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) { Txt("Widgets", T.body); Txt("Home screen widgets, sizes and colors", T.small, muted = true) }
                Txt("›", T.title, muted = true)
            }
        }
        var adv by remember { mutableStateOf(false) }
        TextAction(if (adv) "Hide advanced settings" else "Advanced settings") { adv = !adv }
        if (adv) {
        Card {
            Label("Alerts")
            Spacer(Modifier.height(10.dp))
            Txt("Notify when usage crosses", T.body)
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(50, 75, 90, 100).forEach { v ->
                    Chip("$v%", v in thr) {
                        thr = if (v in thr) thr - v else thr + v
                        Store.setThresholds(c, thr); ask()
                    }
                }
            }
            Spacer(Modifier.height(8.dp))
            Row2("Session reset", "Tell me when a heavy session rolls over") {
                Switch(nreset, { nreset = it; Store.setNotifyReset(c, it); ask() })
            }
            Row2("Live notification", "Pinned bar with session progress") {
                Switch(live, { live = it; Store.setLive(c, it); ask(); Notifier.live(c, Ui.usage) })
            }
        }
        Card {
            Label("Voice")
            Spacer(Modifier.height(10.dp))
            var vlang by remember { mutableStateOf(Store.voiceLang(c)) }
            Txt("Language of calls", T.body)
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Chip("English", vlang == "en") { vlang = "en"; Store.setVoiceLang(c, "en") }
                Chip("Nederlands", vlang == "nl") { vlang = "nl"; Store.setVoiceLang(c, "nl") }
            }
            Spacer(Modifier.height(4.dp))
            Txt("Replies are spoken in the language they are written in. You can switch during a call too.", T.small, muted = true)
            Spacer(Modifier.height(14.dp))
            var engine by remember { mutableStateOf(Store.voiceEngine(c)) }
            var info by remember { mutableStateOf("Checking…") }
            LaunchedEffect(engine) {
                val m = Store.machines(c).firstOrNull()
                info = if (m == null) "Pair a computer first" else withContext(Dispatchers.IO) {
                    runCatching { VoiceApi.status(m) }.fold({ (stt, tts) ->
                        if (stt && tts) "${m.name}: speech engines ready" else "${m.name}: run relay/install_voice.sh to enable local voice"
                    }, { "${m.name} unreachable" })
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("auto" to "Auto", "phone" to "This phone").forEach { (v, l) -> Chip(l, engine == v) { engine = v; Store.setVoiceEngine(c, v) } }
            }
            Spacer(Modifier.height(12.dp))
            var brief by remember { mutableStateOf(Store.voiceBrief(c)) }
            Txt("Spoken replies", T.body)
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Chip("Same as typing", !brief) { brief = false; Store.setVoiceBrief(c, false) }
                Chip("Brief", brief) { brief = true; Store.setVoiceBrief(c, true) }
            }
            Spacer(Modifier.height(8.dp))
            Txt("Same as typing sends exactly what you say and reads Claude's full reply aloud (code is skipped). Brief asks Claude for short spoken answers.", T.small, muted = true)
            Spacer(Modifier.height(10.dp))
            Txt("Auto uses your computer's local models when available (better, slower) and falls back to Android's built-in speech. $info.", T.small, muted = true)
        }
        Card {
            Label("Background refresh")
            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(15, 30, 60).forEach { m ->
                    Chip(if (m == 60) "1 h" else "$m min", interval == m) { interval = m; Store.setInterval(c, m); Sched.ensure(c, replace = true) }
                }
            }
            Spacer(Modifier.height(8.dp))
            Txt("Android may delay background work to save battery. Opening the app always refreshes.", T.small, muted = true)
        }
        }
        Card {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) { Label("Support"); Txt("Found a bug, have a question or an idea? Write to us. We answer by email.", T.small, muted = true) }
                Button("Contact", Modifier) { support = true }
            }
        }
        Txt("Hearth is an independent app, not made by or affiliated with Anthropic. Your usage limits come from what Claude Code reports on your own computer or server; Hearth never sees your Claude login.", T.small, muted = true)
    }
}
