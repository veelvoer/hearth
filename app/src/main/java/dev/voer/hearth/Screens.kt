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
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.graphics.Color
import androidx.compose.material3.Text
import androidx.compose.ui.text.font.FontWeight
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
    private var running = false
    var history by mutableStateOf<List<Sample>>(emptyList())
    var rev by mutableIntStateOf(0)
    var chatOpen by mutableStateOf(false)
    var tab by mutableIntStateOf(0)
    var machinesRev by mutableIntStateOf(0)
    var pendingOpen by mutableStateOf<PendingOpen?>(null)
    var shareRev by mutableIntStateOf(0)
    var pendingJoin by mutableStateOf<String?>(null)   // an invite link that was opened: the shared chats screen shows it

    fun load(c: Context) {
        signedIn = Store.signedIn(c); usage = Store.usage(c); profile = Store.profile(c)
        history = Store.history(c); error = with(Store) { c.error }; rev++
    }

    suspend fun refresh(c: Context, ask: Boolean = false) {
        if (running || !Store.signedIn(c)) return
        running = true; if (ask) busy = true
        try { Repo.refresh(c, ask); load(c) } finally { running = false; busy = false }
    }
}

@Composable
fun tick(): State<Long> = produceState(System.currentTimeMillis()) {
    while (true) { delay(30_000); value = System.currentTimeMillis() }
}

@Composable
fun Screen(content: @Composable ColumnScope.() -> Unit) =
    androidx.compose.foundation.text.selection.SelectionContainer {   // press and hold on any text to select and copy it
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp), content = content)
    }

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
private fun Ring(pct: Float, dia: Dp = 118.dp, stroke: Dp = 11.dp, onClay: Boolean = false) {
    val cs = MaterialTheme.colorScheme
    val track = if (onClay) cs.onPrimary.copy(alpha = .3f) else cs.outline; val fill = if (onClay) cs.onPrimary else if (pct >= 90f) cs.error else cs.primary
    Box(Modifier.size(dia), contentAlignment = Alignment.Center) {
        Canvas(Modifier.fillMaxSize()) {
            val s = stroke.toPx(); val tl = Offset(s / 2, s / 2); val sz = Size(this.size.width - s, this.size.height - s)
            drawArc(track, 0f, 360f, false, tl, sz, style = Stroke(s))
            drawArc(fill, -90f, 360f * (pct / 100f).coerceIn(0f, 1f), false, tl, sz, style = Stroke(s, cap = StrokeCap.Round))
        }
        Row(verticalAlignment = Alignment.Bottom) { Txt(pct.toInt().toString(), T.title, color = if (onClay) cs.onPrimary else Color.Unspecified); Txt("%", T.small, muted = !onClay, color = if (onClay) cs.onPrimary else Color.Unspecified, modifier = Modifier.padding(bottom = 5.dp, start = 1.dp)) }
    }
}

@Composable
private fun Tile(label: String, value: String, sub: String? = null, modifier: Modifier = Modifier) {
    val shape = RoundedCornerShape(18.dp)
    Column(modifier.panel(shape).padding(14.dp)) {
        Txt(label, T.small.copy(fontSize = 12.sp), muted = true)
        Txt(value, T.title.copy(fontSize = 26.sp))
        if (sub != null) Txt(sub, T.small.copy(fontSize = 12.sp), muted = true)
    }
}

@Composable
private fun CostBars(days: List<Double>) {
    val col = MaterialTheme.colorScheme.primary
    Canvas(Modifier.fillMaxWidth().height(96.dp)) {
        val top = (days.maxOrNull() ?: 0.0).coerceAtLeast(0.01)
        val gap = 5.dp.toPx(); val w = ((size.width - gap * (days.size - 1)) / days.size.coerceAtLeast(1)).coerceAtLeast(2f)
        days.forEachIndexed { i, v ->
            val hh = (size.height * (v / top).toFloat()).coerceAtLeast(3.dp.toPx())
            drawRoundRect(col, Offset(i * (w + gap), size.height - hh), Size(w, hh), CornerRadius(w / 2))
        }
    }
}

/** The whole week as one slider: a bar from 0 to 100% of the limit cut into days. The handle between two days moves how much the day on its left gets, the days still to come
 *  share the rest evenly and follow while you drag. The solid part of a day is what is already used. */
@Composable
private fun WeekSlider(p: WeekPlan, onPin: (PlanDay, Double) -> Unit, onTap: (PlanDay) -> Unit) {
    val cs = MaterialTheme.colorScheme
    val base = remember(p) { groupsOf(p.segs, p.targets, p.r) }
    var live by remember(p) { mutableStateOf<Groups?>(null) }
    var drag by remember(p) { mutableIntStateOf(-1) }
    var tip by remember(p) { mutableStateOf("") }
    val res = live ?: base
    val days = res.days
    val dens = LocalDensity.current
    val baseDays by rememberUpdatedState(base.days)
    val txt = cs.onSurfaceVariant; val ink = cs.onSurface
    Column {
        Canvas(Modifier.fillMaxWidth().height(110.dp)
            .pointerInput(p) {
                detectTapGestures { pos ->
                    val w = size.width.toFloat(); val pct = pos.x / w * 100f
                    var at = 0.0
                    baseDays.forEach { d -> if (pct >= at && pct < at + d.planned) { onTap(d); return@detectTapGestures }; at += d.planned }
                }
            }
            .pointerInput(p) {
                var k = -1; var before = 0.0; var d0: PlanDay? = null; var value = 0.0
                detectDragGestures(
                    onDragStart = { pos ->
                        val w = size.width.toFloat(); var at = 0.0; var best = -1; var bestDist = 30.dp.toPx()
                        baseDays.forEachIndexed { i, d -> at += d.planned; if (i < baseDays.size - 1 && d.state != "past") { val dist = kotlin.math.abs(pos.x - (at / 100.0 * w).toFloat()); if (dist < bestDist) { bestDist = dist; best = i } } }
                        k = best
                        if (k >= 0) { d0 = baseDays[k]; before = baseDays.take(k).sumOf { it.planned }; drag = k; value = d0!!.planned }
                    },
                    onDrag = { change, _ ->
                        val d = d0
                        if (k >= 0 && d != null) {
                            change.consume()
                            val x = (change.position.x / size.width.toFloat() * 100.0).coerceIn(0.0, 100.0)
                            value = Math.round(maxOf(d.used, minOf(100.0 - before, x - before)) * 2) / 2.0
                            live = groupsOf(p.segs, p.targets + (d.wd to value), p.r)
                            tip = "${d.name} ${if (value % 1.0 == 0.0) value.toInt().toString() else value.toString()}%"
                        }
                    },
                    onDragEnd = { val d = d0; if (k >= 0 && d != null) onPin(d, value); k = -1; d0 = null; drag = -1 },
                    onDragCancel = { k = -1; d0 = null; drag = -1; live = null },
                )
            }) {
            val w = size.width; val top = 26.dp.toPx(); val hh = 44.dp.toPx(); val rad = 16.dp.toPx()
            val paint = android.graphics.Paint(android.graphics.Paint.ANTI_ALIAS_FLAG).apply { textAlign = android.graphics.Paint.Align.CENTER }
            fun argb(c: androidx.compose.ui.graphics.Color) = android.graphics.Color.argb((c.alpha * 255).toInt(), (c.red * 255).toInt(), (c.green * 255).toInt(), (c.blue * 255).toInt())
            val clip = androidx.compose.ui.graphics.Path().apply { addRoundRect(androidx.compose.ui.geometry.RoundRect(0f, top, w, top + hh, CornerRadius(rad))) }
            clipPath(clip) {
                drawRect(cs.outline.copy(alpha = .55f), Offset(0f, top), Size(w, hh))
                var at = 0.0
                days.forEachIndexed { i, d ->
                    val x0 = (at / 100.0 * w).toFloat(); val sw = (d.planned / 100.0 * w).toFloat()
                    drawRect(cs.primary.copy(alpha = if (d.fixed) .30f else .20f), Offset(x0, top), Size(sw, hh))
                    val uw = if (d.planned > 0) (d.used / d.planned * sw).toFloat().coerceAtMost(sw) else 0f
                    if (uw > 0f) drawRect(cs.primary, Offset(x0, top), Size(uw, hh))
                    if (d.state == "today") drawRect(cs.primary.copy(alpha = .75f), Offset(x0, top), Size(sw, hh), style = Stroke(2.dp.toPx()))
                    if (i > 0) drawRect(cs.surfaceVariant, Offset(x0 - 1.dp.toPx(), top), Size(2.dp.toPx(), hh))
                    at += d.planned
                }
            }
            var at = 0.0
            days.forEachIndexed { i, d ->
                val x0 = (at / 100.0 * w).toFloat(); val sw = (d.planned / 100.0 * w).toFloat()
                if (d.planned >= 5.5) {
                    paint.textSize = 11.sp.toPx(); paint.color = argb(if (d.state == "today") cs.primary else txt); paint.isFakeBoldText = d.state == "today"
                    drawContext.canvas.nativeCanvas.drawText(d.name + if (d.fixed) "•" else "", x0 + sw / 2, top + hh + 16.dp.toPx(), paint)
                    paint.color = argb(ink); paint.isFakeBoldText = true
                    val v = if (d.state == "past") d.used else d.planned
                    drawContext.canvas.nativeCanvas.drawText((if (v >= 10 || v == 0.0) v.toInt().toString() else v.toString()) + "%", x0 + sw / 2, top + hh + 31.dp.toPx(), paint)
                }
                at += d.planned
                if (i < days.size - 1 && d.state != "past") {
                    val kx = (at / 100.0 * w).toFloat(); val kw = 22.dp.toPx(); val kh = 44.dp.toPx(); val active = drag == i
                    val r = androidx.compose.ui.geometry.RoundRect(kx - kw / 2, top + hh / 2 - kh / 2, kx + kw / 2, top + hh / 2 + kh / 2, CornerRadius(12.dp.toPx()))
                    drawRoundRect(androidx.compose.ui.graphics.Color.Black.copy(alpha = .12f), Offset(r.left, r.top + 2.dp.toPx()), Size(kw, kh), CornerRadius(12.dp.toPx()))
                    drawRoundRect(cs.surfaceVariant, Offset(r.left, r.top), Size(kw, kh), CornerRadius(12.dp.toPx()))
                    drawRoundRect(if (d.fixed || active) cs.primary else cs.outline, Offset(r.left, r.top), Size(kw, kh), CornerRadius(12.dp.toPx()), style = Stroke(1.5.dp.toPx()))
                    val gc = if (d.fixed || active) cs.primary else txt
                    drawLine(gc, Offset(kx - 3.dp.toPx(), top + hh / 2 - 8.dp.toPx()), Offset(kx - 3.dp.toPx(), top + hh / 2 + 8.dp.toPx()), 2.dp.toPx(), StrokeCap.Round)
                    drawLine(gc, Offset(kx + 3.dp.toPx(), top + hh / 2 - 8.dp.toPx()), Offset(kx + 3.dp.toPx(), top + hh / 2 + 8.dp.toPx()), 2.dp.toPx(), StrokeCap.Round)
                    if (active && tip.isNotEmpty()) {
                        paint.textSize = 12.sp.toPx(); paint.isFakeBoldText = true
                        val tw = paint.measureText(tip) + 18.dp.toPx(); val tx = kx.coerceIn(tw / 2, w - tw / 2)
                        drawRoundRect(ink, Offset(tx - tw / 2, 0f), Size(tw, 22.dp.toPx()), CornerRadius(8.dp.toPx()))
                        paint.color = argb(cs.surfaceVariant); drawContext.canvas.nativeCanvas.drawText(tip, tx, 15.dp.toPx(), paint)
                    }
                }
            }
        }
        val head = listOfNotNull(
            "${p.left.toInt()}% left" + (if (res.autoDays > 0) " · the ${res.autoDays} day${if (res.autoDays > 1) "s" else ""} without a pin share it evenly, ${res.autoEach}% each" else ""),
            if (res.over) "Your pins are more than what is left, so they are scaled down" else if (res.leftover > 0) "${res.leftover}% is not planned on any day" else null,
            if (p.unknown > 0) "${p.unknown.toInt()}% was used before Hearth started watching this week" else null,
        ).joinToString(". ") + "."
        Txt(head, T.small, muted = true)
    }
}

@Composable
private fun WeekPlanCard() {
    val c = LocalContext.current
    val cs = MaterialTheme.colorScheme
    var plan by remember { mutableStateOf<WeekPlan?>(null) }
    var err by remember { mutableStateOf<String?>(null) }
    var sel by remember { mutableStateOf<PlanDay?>(null) }
    val scope = rememberCoroutineScope()
    val tz = remember { java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 60_000 }
    fun machine(): Machine? { val ms = Store.machines(c); return ms.firstOrNull { it.secure } ?: ms.firstOrNull() }
    LaunchedEffect(Ui.machinesRev) {
        while (true) {
            val m = machine()
            if (m == null) err = "No computer connected." else runCatching { withContext(Dispatchers.IO) { Relay.weekPlan(m, tz) } }.onSuccess { plan = it; err = null }.onFailure { err = "Can't reach your computer right now." }
            delay(30_000)
        }
    }
    fun save(day: PlanDay, pct: Double?) {
        val m = machine() ?: return
        scope.launch {
            runCatching { withContext(Dispatchers.IO) { Relay.setWeekDay(m, day.wd, pct, tz) } }.onSuccess { plan = it; err = null; sel = null }.onFailure { err = it.message ?: "Couldn't save" }
        }
    }
    Card {
        Txt("Your week, day by day", T.body.copy(fontWeight = FontWeight.SemiBold))
        val p = plan
        if (p == null) { Txt(err ?: "Loading…", T.small, muted = true, modifier = Modifier.padding(top = 8.dp)); return@Card }
        if (p.unavailable != null) { Txt(p.unavailable, T.small, muted = true, modifier = Modifier.padding(top = 8.dp)); return@Card }
        Txt("Drag a handle to plan a day. Tap a day to type a number.", T.small, muted = true)
        Spacer(Modifier.height(10.dp))
        WeekSlider(p, onPin = { d, v -> save(d, v) }, onTap = { sel = it })
        Spacer(Modifier.height(4.dp))
        err?.let { Txt(it, T.small, color = cs.error) }
    }
    sel?.let { d ->
        var text by remember(d.wd) { mutableStateOf(((d.target ?: Math.round(d.planned).toDouble())).let { if (it % 1.0 == 0.0) it.toInt().toString() else it.toString() }) }
        val full = listOf("Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday")[d.wd]
        androidx.compose.material3.AlertDialog(
            onDismissRequest = { sel = null },
            title = { Txt(full, T.h2) },
            text = { Column { Txt("Share of the whole week for this day (percent).", T.small, muted = true); Spacer(Modifier.height(8.dp)); Field(text, { text = it.filter { ch -> ch.isDigit() || ch == '.' }.take(5) }, "e.g. 20", number = true) } },
            confirmButton = { TextAction("Pin") { text.toDoubleOrNull()?.takeIf { it in 0.0..100.0 }?.let { save(d, it) } } },
            dismissButton = { Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) { if (d.fixed) TextAction("Share evenly") { save(d, null) }; TextAction("Close") { sel = null } } },
        )
    }
}

@Composable
fun NowScreen() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    val now by tick()
    val u = Ui.usage
    var cost by remember { mutableStateOf<ApiCost?>(null) }
    LaunchedEffect(Ui.machinesRev) {   // what the work is worth: asked once a minute while this screen is open
        while (true) {
            val ms = Store.machines(c)
            val m = ms.firstOrNull { it.secure } ?: ms.firstOrNull()
            if (m != null) runCatching { withContext(Dispatchers.IO) { Relay.cost(m) } }.onSuccess { cost = it }
            delay(60_000)
        }
    }
    Screen {
        val ldt = java.time.LocalDateTime.now()
        Header(when { ldt.hour < 5 -> "Late night"; ldt.hour < 12 -> "Good morning"; ldt.hour < 18 -> "Good afternoon"; else -> "Good evening" },
            listOfNotNull(ldt.format(DateTimeFormatter.ofPattern("EEEE, MMMM d")), Ui.profile?.plan?.takeIf { it.isNotEmpty() }?.let { "Claude $it" }).joinToString(" · ")) {
            TextAction(if (Ui.busy) "Updating…" else "Refresh") { scope.launch { Ui.refresh(c, ask = true) } }
        }
        Ui.error?.let {
            Card { Txt(it, T.small, color = MaterialTheme.colorScheme.primary) }
        }
        val s = u?.session
        val shape = RoundedCornerShape(22.dp)
        Row(Modifier.fillMaxWidth().clip(shape).background(MaterialTheme.colorScheme.primary).padding(20.dp), verticalAlignment = Alignment.CenterVertically) {
            Ring(s?.at(now)?.toFloat() ?: 0f, onClay = true)
            Spacer(Modifier.width(18.dp))
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Txt("SESSION LIMIT", T.label, color = MaterialTheme.colorScheme.onPrimary.copy(alpha = .8f))
                if (s == null) Txt("No data yet", T.body, color = MaterialTheme.colorScheme.onPrimary) else {
                    val r = s.resetsAt
                    Txt(if (r != null && r > now) "Resets in ${dur(r - now)}" else "Fresh window", T.body.copy(fontWeight = FontWeight.SemiBold), color = MaterialTheme.colorScheme.onPrimary)
                    val pc = pace(s, SESSION_MS, now)
                    paceWord(s, pc)?.let { w -> Txt(if (pc?.hitsAt != null) "$w · limit in ${dur(pc.hitsAt - now)}" else w, T.small, color = MaterialTheme.colorScheme.onPrimary.copy(alpha = .85f)) }
                }
            }
        }
        val w = u?.week
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            val wr = w?.resetsAt
            Tile("Week", if (w == null) "–" else "${w.at(now).toInt()}%", if (wr != null && wr > now) "resets in ${dur(wr - now)}" else null, Modifier.weight(1f))
            Tile("API value", cost?.let { "$" + "%,.0f".format(it.total) } ?: "–", "last 30 days", Modifier.weight(1f))
        }
        WeekPlanCard()
        cost?.takeIf { it.days.isNotEmpty() }?.let { cc ->
            Card {
                Txt("Cost per day", T.body.copy(fontWeight = FontWeight.SemiBold))
                Spacer(Modifier.height(12.dp))
                CostBars(cc.days)
            }
        }
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
        val ok = u != null && Ui.error == null
        Row(Modifier.fillMaxWidth().panel(RoundedCornerShape(18.dp)).padding(horizontal = 16.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(7.dp).clip(RoundedCornerShape(50)).background(if (ok) androidx.compose.ui.graphics.Color(0xFF4CAF7D) else MaterialTheme.colorScheme.onSurfaceVariant))
            Spacer(Modifier.width(10.dp))
            Txt(if (u == null) "Waiting for the first numbers" else if (ok) "Online · updated ${ago(u.at, now)}" else "Can't reach your computer right now", T.small, muted = true)
        }
        if (u != null) Txt("Same limits as /usage in Claude Code.", T.small, muted = true)
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

fun modelName(id: String): String? = Regex("(opus|sonnet|haiku)-(\\d+)(?:-(\\d{1,2}))?(?!\\d)", RegexOption.IGNORE_CASE).find(id)?.let { m ->
    m.groupValues[1].replaceFirstChar { it.uppercase() } + " " + m.groupValues[2] + (m.groupValues[3].takeIf { it.isNotEmpty() }?.let { ".$it" } ?: "") }
fun tokensShort(n: Long) = when { n >= 1_000_000 -> "%.1fM".format(n / 1e6); n >= 1_000 -> "${n / 1000}k"; else -> n.toString() }

@Composable
fun HistoryScreen() {
    var range by remember { mutableIntStateOf(0) }
    val now by tick()
    val spans = listOf(24L * 3600_000, 7L * 86_400_000, 30L * 86_400_000)
    val from = now - spans[range]
    val data = Ui.history.filter { it.t >= from }
    val c = LocalContext.current
    var chats by remember { mutableStateOf<List<Sess>>(emptyList()) }
    var filter by remember { mutableIntStateOf(0) }
    LaunchedEffect(Ui.machinesRev) {   // every chat from every computer, refreshed while this screen is open
        while (true) {
            val got = Store.machines(c).flatMap { m -> runCatching { withContext(Dispatchers.IO) { Relay.sessions(m) } }.getOrDefault(emptyList()) }
            if (got.isNotEmpty() || chats.isEmpty()) chats = got.groupBy { it.id }.values.map { g -> g.maxByOrNull { it.mtime }!! }
            delay(20_000)
        }
    }
    Screen {
        Header("History", "Every chat, every computer")
        ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("All", "This week", "Costliest").forEachIndexed { i, s -> Chip(s, filter == i) { filter = i } }
        }
        val startToday = java.time.LocalDate.now().atStartOfDay(ZoneId.systemDefault()).toInstant().toEpochMilli()
        val shown = chats.filter { filter != 1 || it.mtime >= now - 7 * 86_400_000L }.let { l -> if (filter == 2) l.sortedByDescending { it.tokens }.take(30) else l.sortedByDescending { it.mtime } }.take(80)
        val groups = shown.groupBy { when { filter == 2 -> "Costliest"; it.mtime >= startToday -> "Today"; it.mtime >= startToday - 86_400_000L -> "Yesterday"; it.mtime >= now - 7 * 86_400_000L -> "This week"; else -> "Earlier" } }
        groups.forEach { (g, items) ->
            Label(g)
            items.forEach { x ->
                val work = x.running || x.busy
                val shape = RoundedCornerShape(18.dp)
                Row(Modifier.fillMaxWidth().panel(shape).padding(horizontal = 16.dp, vertical = 13.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(9.dp).clip(RoundedCornerShape(50)).background(if (work) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline))
                    Spacer(Modifier.width(14.dp))
                    Column(Modifier.weight(1f)) {
                        Text(x.title, style = T.body.copy(fontWeight = FontWeight.SemiBold), color = MaterialTheme.colorScheme.onSurface, maxLines = 1, overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis)
                        Txt(listOfNotNull(x.cwd.trimEnd('/').substringAfterLast('/').ifEmpty { null }, modelName(x.model), if (work) "working now" else dur(now - x.mtime) + " ago").joinToString(" · "), T.small.copy(fontSize = 12.sp), muted = true)
                    }
                    if (x.tokens > 0) Txt(tokensShort(x.tokens), T.small, muted = true)
                }
            }
        }
        if (chats.isEmpty()) Txt("No chats yet.", T.small, muted = true)
        Spacer(Modifier.height(6.dp))
        Txt("Limits over time", T.h2)
        Txt("Sampled every time the app or a widget refreshes", T.small, muted = true)
        ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
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
fun UsageScreen() = Box(Modifier.fillMaxSize().tourTarget("usage")) { NowScreen() }

@Composable
private fun SyncCard() {
    val c = LocalContext.current
    var text by remember { mutableStateOf("Checking…") }
    LaunchedEffect(Ui.machinesRev) {
        while (true) {
            val ms = Store.machines(c); val m = ms.firstOrNull { it.secure } ?: ms.firstOrNull()
            text = if (m == null) "Connect a computer or server first." else runCatching { withContext(Dispatchers.IO) { "${m.name}: " + Relay.syncText(m) } }.getOrElse { "${m.name} can't be reached right now." }
            delay(30_000)
        }
    }
    Card { Head("sync", "Sync", "Chats and project files that the server keeps the same on all your computers."); Spacer(Modifier.height(8.dp)); Txt(text, T.small, muted = true) }
}

/** What the server may do for you: work in any folder, Full auto, chats with root rights, and the commands allowed for good. */
@Composable
private fun PermissionsCard() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var o by remember { mutableStateOf<Relay.Options?>(null) }
    var msg by remember { mutableStateOf("Checking…") }
    var cmd by remember { mutableStateOf("") }
    fun machine(): Machine? { val ms = Store.machines(c); return ms.firstOrNull { it.secure } ?: ms.firstOrNull() }
    LaunchedEffect(Ui.machinesRev) {
        val m = machine()
        if (m == null) msg = "Connect a computer or server first." else runCatching { withContext(Dispatchers.IO) { Relay.options(m) } }.onSuccess { o = it; msg = "" }.onFailure { msg = "This server has no permission settings yet. Update it first." }
    }
    fun save(body: org.json.JSONObject) {
        val m = machine() ?: return
        scope.launch { runCatching { withContext(Dispatchers.IO) { Relay.setOptions(m, body) } }.onSuccess { o = it; msg = "" }.onFailure { msg = it.message ?: "Couldn't save" } }
    }
    Card {
        Head("server", "Server permissions", "Choose how much Claude may do on your server. Nothing here applies to your own computers.")
        Spacer(Modifier.height(10.dp))
        val v = o
        if (v == null) { Txt(msg, T.small, muted = true); return@Card }
        if (v.outsideAvailable) {
            Row(verticalAlignment = Alignment.CenterVertically) { Column(Modifier.weight(1f)) { Txt("Work outside the projects folder", T.body); Txt("Browse the whole server and start a chat in any folder.", T.small, muted = true) }; Switch(v.outside, { save(org.json.JSONObject().put("outside", it)) }) }
            Spacer(Modifier.height(10.dp))
        }
        Row(verticalAlignment = Alignment.CenterVertically) { Column(Modifier.weight(1f)) { Txt("Allow Full auto", T.body); Txt(if (v.bypassFixed) "Always on for this server." else "Claude may run anything in a chat set to Full auto, without asking.", T.small, muted = true) }; if (!v.bypassFixed) Switch(v.bypass, { save(org.json.JSONObject().put("bypass", it)) }) }
        Spacer(Modifier.height(10.dp))
        Row(verticalAlignment = Alignment.CenterVertically) { Column(Modifier.weight(1f)) { Txt("Allow chats with root rights", T.body); Txt("A chat can run as root through sudo, and Claude is told it is root.", T.small, muted = true) }; Switch(v.root, { save(org.json.JSONObject().put("root", it)) }) }
        if (v.root && !v.rootOk) { Spacer(Modifier.height(6.dp)); Txt("Not possible yet: " + v.rootWhy, T.small, color = MaterialTheme.colorScheme.error) }
        Spacer(Modifier.height(14.dp))
        Txt("Commands allowed for good", T.body.copy(fontWeight = FontWeight.SemiBold))
        if (v.rules.isEmpty()) Txt("None yet. When Claude asks to run a command, choose \"Yes, and don't ask again\".", T.small, muted = true)
        v.rules.forEach { r ->
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 6.dp)) {
                Txt(r.label, T.small.copy(fontFamily = FontFamily.Monospace), modifier = Modifier.weight(1f))
                TextAction("Remove") { save(org.json.JSONObject().put("removeRule", r.id)) }
            }
        }
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Box(Modifier.weight(1f)) { Field(cmd, { cmd = it }, "Add a command, e.g. npm test", mono = true) }
            TextAction("Add") { if (cmd.isNotBlank()) { save(org.json.JSONObject().put("addPrefix", cmd.trim())); cmd = "" } }
        }
        if (msg.isNotEmpty()) Txt(msg, T.small, color = MaterialTheme.colorScheme.error)
    }
}

@Composable
private fun MemoryCard() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var info by remember { mutableStateOf<Relay.MemInfo?>(null) }
    var picked by remember { mutableStateOf<Int?>(null) }
    var msg by remember { mutableStateOf("Checking…") }
    fun machine(): Machine? { val ms = Store.machines(c); return ms.firstOrNull { it.secure } ?: ms.firstOrNull() }
    LaunchedEffect(Ui.machinesRev) {
        val m = machine()
        if (m == null) msg = "Connect a computer or server first." else runCatching { withContext(Dispatchers.IO) { Relay.memory(m) } }.onSuccess { info = it; msg = "" }.onFailure { msg = "This server has no memory setting yet. Update it first." }
    }
    Card {
        Head("server", "Server memory", "The most memory the server program may use, as a share of the memory that is free when it starts. 75% is the default. It takes effect when the server starts again.")
        Spacer(Modifier.height(10.dp))
        val i = info
        if (i == null) { Txt(msg, T.small, muted = true); return@Card }
        val cur = picked ?: i.pct
        ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(25 to "25%", 50 to "50%", 75 to "75% (default)", 90 to "90%", 0 to "No limit").forEach { (v, l) -> Chip(l, cur == v) { picked = v } }
        }
        Spacer(Modifier.height(8.dp))
        val want = if (cur == 0) null else maxOf(256, minOf(i.availableMb * cur / 100, (i.totalMb * 0.9).toInt()))
        Txt("${i.availableMb} MB free of ${i.totalMb} MB. Using ${i.usedMb} MB now. " + (if (want != null) "Limit for $cur%: about $want MB." else "No limit of its own."), T.small, muted = true)
        if (picked != null && picked != i.pct) {
            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                Button(if (i.canRestart) "Save and restart" else "Save") {
                    val m = machine() ?: return@Button; val v = picked ?: return@Button
                    scope.launch { runCatching { withContext(Dispatchers.IO) { Relay.setMemory(m, v, i.canRestart) } }.onSuccess { info = i.copy(pct = v); picked = null; msg = "" }.onFailure { msg = it.message ?: "Couldn't save" } }
                }
                TextAction("Cancel") { picked = null }
            }
        }
        if (msg.isNotEmpty()) Txt(msg, T.small, color = MaterialTheme.colorScheme.error)
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
        Card { Head("tour", "Help", "The tour lights up each part and explains it. Setup walks you through connecting a server or computer again.") { TextAction("Tour") { Tour.start() }; Spacer(Modifier.width(14.dp)); TextAction("Setup") { Store.setOnboarded(c, false); Ui.tourRev++ } } }
        Card {
            Head("bell", "Notifications and calls", "Messages when a session finishes or needs you, even when the app is closed. Calls only when you ask for one in a chat. Allow background activity so Android never puts the connection to sleep. Calls need the full-screen permission on Android 14 and newer.")
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
        }
        Card {
            Head("paint", "Appearance", if (Build.VERSION.SDK_INT >= 31) "System colors uses your phone's wallpaper colors for the whole app. Light and dark always follow your phone." else "Light and dark follow your phone.")
            Spacer(Modifier.height(10.dp))
            val sys = ThemePrefs.system == true
            ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Chip("Claude", !sys) { ThemePrefs.system = false; Store.setThemeSystem(c, false) }
                if (Build.VERSION.SDK_INT >= 31) Chip("System colors", sys) { ThemePrefs.system = true; Store.setThemeSystem(c, true) }
            }
            Spacer(Modifier.height(10.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) { Txt("Frosted glass", T.body); Txt("See-through panels. Turn off if the app feels slow.", T.small, muted = true) }
                Switch(Fx.glass, { Fx.glass = it; Store.setGlass(c, it) })
            }
            Spacer(Modifier.height(10.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) { Txt("Animations", T.body); Txt("Fades, slides and bounces. Off is the lightest.", T.small, muted = true) }
                Switch(Fx.motion, { Fx.motion = it; Store.setMotion(c, it) })
            }
            Spacer(Modifier.height(8.dp))
        }
        Card(Modifier.clickable { sub = "widgets" }) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) { Txt("Widgets", T.body); Txt("Home screen widgets, sizes and colors", T.small, muted = true) }
                Txt("›", T.title, muted = true)
            }
        }
        Card {
            Head("bell", "Alerts")
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
            Head("voice", "Voice", "Replies are spoken in the language they are written in; you can switch during a call too. Same as typing sends exactly what you say and reads Claude's full reply aloud (code is skipped). Brief asks Claude for short spoken answers. Auto uses your computer's local voices when available (better, slower) and falls back to Android's built-in speech.")
            Spacer(Modifier.height(10.dp))
            var vlang by remember { mutableStateOf(Store.voiceLang(c)) }
            Txt("Language of calls", T.body)
            Spacer(Modifier.height(8.dp))
            ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Chip("English", vlang == "en") { vlang = "en"; Store.setVoiceLang(c, "en") }
                Chip("Nederlands", vlang == "nl") { vlang = "nl"; Store.setVoiceLang(c, "nl") }
            }
            Spacer(Modifier.height(4.dp))
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
            ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("auto" to "Auto", "phone" to "This phone").forEach { (v, l) -> Chip(l, engine == v) { engine = v; Store.setVoiceEngine(c, v) } }
            }
            Spacer(Modifier.height(12.dp))
            var brief by remember { mutableStateOf(Store.voiceBrief(c)) }
            Txt("Spoken replies", T.body)
            Spacer(Modifier.height(8.dp))
            ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Chip("Same as typing", !brief) { brief = false; Store.setVoiceBrief(c, false) }
                Chip("Brief", brief) { brief = true; Store.setVoiceBrief(c, true) }
            }
            Spacer(Modifier.height(8.dp))
            Spacer(Modifier.height(10.dp))
        }
        Card {
            Head("clock", "Background refresh", "Android may delay background work to save battery. Opening the app always refreshes.")
            Spacer(Modifier.height(10.dp))
            ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(15, 30, 60).forEach { m ->
                    Chip(if (m == 60) "1 h" else "$m min", interval == m) { interval = m; Store.setInterval(c, m); Sched.ensure(c, replace = true) }
                }
            }
            Spacer(Modifier.height(8.dp))
        }
        SyncCard()
        MemoryCard()
        PermissionsCard()
        Card {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Head("support", "Support", "Found a bug, have a question or an idea? Write to us. We answer by email.", Modifier.weight(1f))
                Button("Contact", Modifier) { support = true }
            }
            Spacer(Modifier.height(10.dp))
            val warn = RoundedCornerShape(12.dp)
            Column(Modifier.fillMaxWidth().clip(warn).background(MaterialTheme.colorScheme.primary.copy(alpha = .10f)).border(1.dp, MaterialTheme.colorScheme.primary.copy(alpha = .35f), warn).padding(12.dp)) {
                Txt("If support does not work from here, write an email yourself to hearth.support1@gmail.com. Say what you did and what you saw.", T.small)
                Spacer(Modifier.height(6.dp))
                TextAction("Write an email") { runCatching { c.startActivity(Intent(Intent.ACTION_SENDTO, Uri.parse("mailto:hearth.support1@gmail.com"))) } }
            }
        }
        Txt("Hearth is an independent app, not made by or affiliated with Anthropic. Your usage limits come from what Claude Code reports on your own computer or server; Hearth never sees your Claude login.", T.small, muted = true)
    }
}
