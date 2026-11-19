package dev.voer.hearth.wear

import android.content.Intent
import android.content.pm.ApplicationInfo
import android.os.Bundle
import android.speech.RecognizerIntent
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.gestures.scrollBy
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.foundation.focusable
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.rotary.onRotaryScrollEvent
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.foundation.text.BasicText
import androidx.compose.ui.text.TextStyle
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/*
 * Made for the round screen and for a slow watch: nothing but plain Compose (no Wear Material, no scaling lists, no clock
 * overlay), every screen laid out inside the circle (content width follows the curve), cheap press feedback on the GPU
 * and polling that only runs while the app is on screen.
 */
private val Clay get() = Pal.cur.clay
private val Ink get() = Pal.cur.ink
private val Muted get() = Pal.cur.muted
private val Card get() = Pal.cur.card
private val Bg get() = Pal.cur.bg

/** Polling stops while the watch app is not on screen. */
object Visible { @Volatile var now = true }

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Phone.app = applicationContext
        Pal.load(applicationContext); Update.cleanup(applicationContext)
        testPair(intent)
        setContent { WatchApp() }
    }
    override fun onNewIntent(intent: Intent) { super.onNewIntent(intent); testPair(intent) }
    override fun onResume() { super.onResume(); Visible.now = true }
    override fun onPause() { Visible.now = false; super.onPause() }

    /** Test builds only: servers can be handed over with an intent, to try the app without a paired phone. */
    private fun testPair(i: Intent?) {
        if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE == 0 && !packageName.endsWith(".bench")) return
        i?.getStringExtra("machines")?.let { Servers.save(this, it) }
    }
}

private suspend fun pause(ms: Long) { delay(ms); while (!Visible.now) delay(500) }

// ───────────────────────── small building blocks ─────────────────────────

private fun t(size: Int, color: Color, weight: FontWeight = FontWeight.Normal, align: TextAlign = TextAlign.Center) = TextStyle(fontSize = size.sp, color = color, fontWeight = weight, textAlign = align, lineHeight = (size + 4).sp)

@Composable
private fun Label(text: String, size: Int = 13, color: Color = Ink, weight: FontWeight = FontWeight.Normal, lines: Int = Int.MAX_VALUE, modifier: Modifier = Modifier, align: TextAlign = TextAlign.Center) =
    BasicText(text, modifier, style = t(size, color, weight, align), maxLines = lines, overflow = TextOverflow.Ellipsis)

/** A button: fills with the surface or the accent, shrinks a little while pressed (one GPU property, no relayout). */
@Composable
private fun Pill(modifier: Modifier = Modifier, primary: Boolean = false, enabled: Boolean = true, desc: String? = null, onClick: () -> Unit, content: @Composable () -> Unit) {
    var down by remember { mutableStateOf(false) }
    val s by animateFloatAsState(if (down) 0.94f else 1f, spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium), label = "press")
    Box(modifier.heightMin().graphicsLayer { scaleX = s; scaleY = s; alpha = if (enabled) 1f else 0.5f }
        .clip(RoundedCornerShape(24.dp)).background(if (primary) Clay else Card)
        .semantics { role = Role.Button; if (desc != null) contentDescription = desc }
        .pointerInput(enabled) { detectTapGestures(onPress = { down = true; tryAwaitRelease(); down = false }, onTap = { if (enabled) onClick() }) }
        .padding(horizontal = 8.dp, vertical = 8.dp), contentAlignment = Alignment.Center) { content() }
}
private fun Modifier.heightMin() = this.then(Modifier.heightIn(min = 44.dp))

/** The bezel (and crown) scrolls a list or a column. */
@Composable
private fun Modifier.bezel(scroll: (Float) -> Unit): Modifier {
    val fr = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { fr.requestFocus() } }
    return this.onRotaryScrollEvent { scroll(it.verticalScrollPixels); true }.focusRequester(fr).focusable()
}

/** The width of the round screen at a height: content placed there must stay inside this, so nothing touches the edge. */
private fun chord(h: Float, r: Float): Float { val dy = kotlin.math.abs(h - r); return if (dy >= r) 0f else 2f * kotlin.math.sqrt(r * r - dy * dy) }

// ───────────────────────── app ─────────────────────────

@Composable
fun WatchApp() {
    val c = LocalContext.current
    Box(Modifier.fillMaxSize().background(Bg)) {
        val rev = Servers.rev
        val servers = remember(rev) { Servers.all(c) }
        LaunchedEffect(Unit) { Servers.fetchFromPhone(c) }
        var open by remember { mutableStateOf<Chat?>(null) }
        var screen by remember { mutableStateOf("home") }   // home, chats, settings
        BackHandler(open != null || screen != "home") { if (open != null) open = null else screen = "home" }
        val o = open
        when {
            servers.isEmpty() -> NotPaired()
            o != null -> ChatScreen(o)
            screen == "settings" -> SettingsScreen(servers)
            screen == "chats" -> ChatsScreen(servers) { open = it }
            else -> Home(servers, { screen = "chats" }, { screen = "settings" })
        }
    }
}

@Composable
private fun NotPaired() {
    Box(Modifier.fillMaxSize().padding(horizontal = 34.dp), contentAlignment = Alignment.Center) {
        Label("Open Hearth on your phone once. Your servers come over by themselves.", 14, Ink)
    }
}

/** Errors in a few plain words: a watch has no room for stack traces. */
private fun friendly(e: Throwable?, name: String): String = when {
    e is java.io.IOException && (e.message ?: "").let { it.contains("connect again") || it.contains("busy") || it.contains(" said ") } -> e.message ?: ""
    e is java.net.UnknownHostException || e is java.net.SocketTimeoutException || e is java.net.ConnectException -> "Can't reach $name"
    else -> "Can't reach $name right now"
}

private fun pct(v: Double?) = v?.let { "${it.toInt()}%" } ?: "–"

private fun ago(ms: Long): String {
    val m = ((System.currentTimeMillis() - ms) / 60_000).coerceAtLeast(0)
    return when { m < 1 -> "just now"; m < 60 -> "${m}m ago"; m < 1440 -> "${m / 60}h ago"; else -> "${m / 1440}d ago" }
}

// ───────────────────────── home: what Claude does, at a glance ─────────────────────────

@Composable
private fun Home(servers: List<Server>, onChats: () -> Unit, onSettings: () -> Unit) {
    var usage by remember { mutableStateOf<Usage?>(null) }
    var working by remember { mutableStateOf(0) }
    var err by remember { mutableStateOf("") }
    var asks by remember { mutableStateOf<List<Ask>>(emptyList()) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(servers) {
        while (true) {
            val r = withContext(Dispatchers.IO) {
                val u = servers.firstNotNullOfOrNull { s -> runCatching { Api.usage(s) }.getOrNull() }
                val l = servers.map { s -> runCatching { Api.chats(s) } }
                val a = servers.flatMap { s -> runCatching { Api.pending(s) }.getOrDefault(emptyList()) }
                Triple(u, l.flatMap { it.getOrDefault(emptyList()) }.count { it.busy } to a, (if (l.all { it.isFailure }) friendly(l.first().exceptionOrNull(), servers.first().name) else null))
            }
            usage = r.first ?: usage; working = r.second.first; asks = r.second.second; err = r.third ?: ""
            pause(if (working > 0 || asks.isNotEmpty()) 4_000 else 10_000)
        }
    }
    val p = ((usage?.session ?: 0.0) / 100.0).toFloat().coerceIn(0f, 1f)
    val shown by animateFloatAsState(p, spring(stiffness = Spring.StiffnessLow), label = "ring")
    val hot = (usage?.session ?: 0.0) >= 90
    Box(Modifier.fillMaxSize()) {
        // the ring follows the edge of the screen: usage of the 5-hour window
        Canvas(Modifier.fillMaxSize().padding(5.dp)) {
            val w = 6.dp.toPx(); val r = Size(size.width - w, size.height - w); val tl = Offset(w / 2, w / 2)
            drawArc(Card, 135f, 270f, false, tl, r, style = Stroke(w, cap = StrokeCap.Round))
            if (shown > 0.005f) drawArc(if (hot) Color(0xFFE5484D) else Clay, 135f, 270f * shown, false, tl, r, style = Stroke(w, cap = StrokeCap.Round))
        }
        val ask = asks.firstOrNull()
        if (ask != null) AskCard(ask, scope) { asks = asks.filter { it.req != ask.req } }
        else Column(Modifier.fillMaxSize().padding(horizontal = 40.dp, vertical = 30.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
            Label(pct(usage?.session), 38, Ink, FontWeight.SemiBold)
            val reset = usage?.sessionReset?.takeIf { it > System.currentTimeMillis() }?.let { java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(java.util.Date(it)) }
            Label(if (reset != null) "resets $reset" else "session", 12, Muted)
            Label("week ${pct(usage?.week)}", 12, Muted)
            Spacer(Modifier.height(8.dp))
            if (err.isNotEmpty()) Label(err, 11, Clay, lines = 2)
            else Label(if (working > 0) "$working working…" else "all quiet", 12, if (working > 0) Clay else Muted)
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                Pill(Modifier.width(72.dp), primary = true, desc = "Chats", onClick = onChats) { Label("Chats", 13, Pal.cur.onClay, FontWeight.SemiBold) }
                Pill(Modifier.width(44.dp), desc = "Settings", onClick = onSettings) { Canvas(Modifier.size(18.dp)) { val w = 2.dp.toPx(); for (i in 0..2) { val y = size.height * (0.2f + 0.3f * i); drawLine(Ink, Offset(0f, y), Offset(size.width, y), w, StrokeCap.Round) } } }
            }
        }
    }
}

/** Claude needs you: allow or deny, or pick an answer. Shown instead of the numbers, so it cannot be missed. */
@Composable
private fun AskCard(q: Ask, scope: kotlinx.coroutines.CoroutineScope, done: () -> Unit) {
    val opts = if (q.kind == "permission") listOf("Allow" to "allow", "Deny" to "deny") else q.options.take(3).map { it to it }
    Column(Modifier.fillMaxSize().padding(horizontal = 30.dp, vertical = 34.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
        Label(if (q.kind == "permission") "Claude needs your OK" else "Claude has a question", 11, Clay, FontWeight.SemiBold)
        Spacer(Modifier.height(4.dp))
        Label(q.text.ifBlank { q.title }.take(120), 12, Ink, lines = 4)
        Spacer(Modifier.height(8.dp))
        if (q.kind == "permission") Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            opts.forEach { (label, v) -> Pill(Modifier.width(66.dp), primary = v == "allow", onClick = { scope.launch { withContext(Dispatchers.IO) { runCatching { Api.decide(q.server, q.req, q.kind, v) } }; done() } }) { Label(label, 13, if (v == "allow") Pal.cur.onClay else Ink, FontWeight.SemiBold) } }
        } else Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            opts.forEach { (label, v) -> Pill(Modifier.width(120.dp), onClick = { scope.launch { withContext(Dispatchers.IO) { runCatching { Api.decide(q.server, q.req, q.kind, v) } }; done() } }) { Label(label, 12, Ink, lines = 1) } }
        }
    }
}

// ───────────────────────── chats ─────────────────────────

@Composable
private fun ChatsScreen(servers: List<Server>, onOpen: (Chat) -> Unit) {
    var chats by remember { mutableStateOf<List<Chat>?>(null) }
    LaunchedEffect(servers) {
        while (true) {
            chats = withContext(Dispatchers.IO) { servers.flatMap { s -> runCatching { Api.chats(s) }.getOrDefault(emptyList()) }.sortedByDescending { it.mtime }.take(12) }
            pause(15_000)
        }
    }
    val state = rememberLazyListState(); val scope = rememberCoroutineScope()
    BoxWithConstraints(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize().bezel { scope.launch { state.scrollBy(it) } }, state = state, contentPadding = PaddingValues(top = 36.dp, bottom = 40.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
            val l = chats
            if (l == null) item { Label("Loading…", 12, Muted) }
            else if (l.isEmpty()) item { Label("No chats yet", 12, Muted) }
            else items(l, key = { it.server.base + it.id }) { ch ->
                Pill(Modifier.fillMaxWidth(0.74f).heightMin().then(Modifier), onClick = { onOpen(ch) }) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Label(ch.title, 12, Ink, FontWeight.Medium, lines = 2)
                        Label(if (ch.busy) "working…" else ago(ch.mtime), 10, if (ch.busy) Clay else Muted, lines = 1)
                    }
                }
            }
        }
    }
}

// ───────────────────────── one chat ─────────────────────────

@Composable
private fun ChatScreen(ch: Chat) {
    val scope = rememberCoroutineScope()
    var turn by remember { mutableStateOf<Turn?>(null) }
    var busy by remember { mutableStateOf(ch.busy) }
    var note by remember { mutableStateOf("") }
    var tick by remember { mutableStateOf(0) }
    LaunchedEffect(ch.id, tick) {
        while (true) {
            withContext(Dispatchers.IO) { runCatching { Api.lastTurn(ch.server, ch.id) to Api.chats(ch.server).firstOrNull { it.id == ch.id }?.busy } }
                .onSuccess { (tn, b) -> turn = tn; busy = b ?: false; note = "" }.onFailure { note = friendly(it, ch.server.name) }
            pause(if (busy) 4_000 else 15_000)
        }
    }
    val speak = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val said = r.data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)?.firstOrNull()?.trim().orEmpty()
        if (said.isNotEmpty()) scope.launch {
            note = "Sending…"
            withContext(Dispatchers.IO) { runCatching { Api.send(ch.server, ch.id, said) } }
                .onSuccess { busy = true; note = ""; turn = Turn(said, ""); delay(1500); tick++ }.onFailure { note = "Not sent. " + friendly(it, ch.server.name) }
        }
    }
    val sc = rememberScrollState()
    Column(Modifier.fillMaxSize().bezel { scope.launch { sc.scrollBy(it) } }.verticalScroll(sc).padding(top = 34.dp, bottom = 46.dp, start = 30.dp, end = 30.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Label(ch.title, 12, Muted, FontWeight.SemiBold, lines = 2)
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Pill(Modifier.width(if (busy) 74.dp else 120.dp), primary = true, enabled = !busy, desc = "Reply by voice", onClick = {
                runCatching { speak.launch(Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM).putExtra(RecognizerIntent.EXTRA_PROMPT, "Say it to Claude")) }.onFailure { note = "No voice input on this watch" }
            }) { Label("Reply", 13, Pal.cur.onClay, FontWeight.SemiBold) }
            if (busy) Pill(Modifier.width(70.dp), desc = "Stop", onClick = { scope.launch { withContext(Dispatchers.IO) { runCatching { Api.stop(ch.server, ch.id) } }; delay(800); tick++ } }) { Label("Stop", 13, Ink) }
        }
        Spacer(Modifier.height(10.dp))
        val tn = turn
        if (tn == null) Label("Loading…", 12, Muted)
        else {
            if (tn.asked.isNotBlank()) { Label(plain(tn.asked).take(140), 11, Muted, lines = 3); Spacer(Modifier.height(8.dp)) }
            if (busy) Label("Claude is working…", 13, Clay)
            else if (tn.answer.isNotBlank()) Label(plain(tn.answer).take(1500), 14, Ink, align = TextAlign.Start, modifier = Modifier.fillMaxWidth())
        }
        if (note.isNotEmpty()) { Spacer(Modifier.height(6.dp)); Label(note, 11, Clay) }
        if (!busy && turn?.answer?.isNotBlank() == true) {
            Spacer(Modifier.height(10.dp))
            Column(verticalArrangement = Arrangement.spacedBy(6.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                listOf("Yes, go ahead", "Continue", "Explain shorter").forEach { q ->
                    Pill(Modifier.width(138.dp), onClick = { scope.launch { note = "Sending…"; withContext(Dispatchers.IO) { runCatching { Api.send(ch.server, ch.id, q) } }.onSuccess { busy = true; note = ""; turn = Turn(q, ""); delay(1500); tick++ }.onFailure { note = "Not sent. " + friendly(it, ch.server.name) } } }) { Label(q, 12, Ink, lines = 1) }
                }
            }
        }
    }
}

// ───────────────────────── settings ─────────────────────────

@Composable
private fun SettingsScreen(servers: List<Server>) {
    val c = LocalContext.current; val scope = rememberCoroutineScope()
    LaunchedEffect(servers) {   // the computer's own look, when one of the servers has it
        val got = withContext(Dispatchers.IO) { servers.firstNotNullOfOrNull { s -> runCatching { Api.theme(s) }.getOrNull() } }
        if (got != null) Pal.savePc(c, got)
    }
    val state = rememberLazyListState()
    LazyColumn(Modifier.fillMaxSize().bezel { scope.launch { state.scrollBy(it) } }, state = state, contentPadding = PaddingValues(top = 38.dp, bottom = 46.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
        item { Label("Theme", 12, Muted, FontWeight.SemiBold) }
        fun row(id: String, label: String, sub: String?, dot: Color?) = item(key = id) {
            val on = Pal.choice == id
            Pill(Modifier.fillMaxWidth(0.72f), primary = on, onClick = { Pal.pick(c, id) }) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    if (dot != null) Box(Modifier.size(12.dp).clip(CircleShape).background(dot))
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Label(label, 12, if (on) Pal.cur.onClay else Ink, FontWeight.Medium, lines = 1)
                        if (sub != null) Label(sub, 9, if (on) Pal.cur.onClay else Muted, lines = 1)
                    }
                }
            }
        }
        row("watch", "My watch", "Hearth's own look", null)
        row("pc", "My computer", if (Pal.pcName.isNotBlank()) Pal.pcName else "not shared yet", null)
        Pal.PRESETS.forEach { row(it.name, it.name, null, it.clay) }
        item { Spacer(Modifier.height(4.dp)); Label("Updates", 12, Muted, FontWeight.SemiBold) }
        item { Label("Hearth ${Update.current(c)}" + (if (Update.status.isNotBlank()) "\n" + Update.status else ""), 11, Muted, lines = 3, modifier = Modifier.fillMaxWidth(0.7f)) }
        item { Pill(Modifier.fillMaxWidth(0.72f), enabled = !Update.busy, onClick = { if (!Update.busy) scope.launch { Update.check(c) } }) { Label("Check for updates", 12, Ink, lines = 1) } }
        item { Pill(Modifier.fillMaxWidth(0.72f), enabled = !Update.busy, onClick = { if (!Update.busy) scope.launch { Update.install(c, face = true) } }) { Label("Install watch face", 12, Ink, lines = 1) } }
        if (Update.ready(c)) item { Pill(Modifier.fillMaxWidth(0.72f), primary = true, enabled = !Update.busy, onClick = { if (!Update.busy) scope.launch { Update.install(c) } }) { Label("Update to ${Update.latest}", 12, Pal.cur.onClay, FontWeight.SemiBold, lines = 1) } }
    }
}
