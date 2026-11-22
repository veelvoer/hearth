package dev.voer.hearth.wear

import android.os.Build
import android.view.HapticFeedbackConstants
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.gestures.ScrollableState
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import kotlinx.coroutines.Job
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.os.Bundle
import android.speech.RecognizerIntent
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.Spring
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.ui.unit.IntOffset
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
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
import androidx.compose.foundation.lazy.itemsIndexed
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

/** The last turn of each chat that was opened: a chat opens with what it showed last time, the new answer replaces it. */
private val turns = java.util.concurrent.ConcurrentHashMap<String, Turn>()

private suspend fun pause(ms: Long) { delay(ms); while (!Visible.now) delay(500) }

// ───────────────────────── small building blocks ─────────────────────────

private fun t(size: Int, color: Color, weight: FontWeight = FontWeight.Normal, align: TextAlign = TextAlign.Center) = TextStyle(fontSize = size.sp, color = color, fontWeight = weight, textAlign = align, lineHeight = (size + 4).sp)

@Composable
private fun Label(text: String, size: Int = 13, color: Color = Ink, weight: FontWeight = FontWeight.Normal, lines: Int = Int.MAX_VALUE, modifier: Modifier = Modifier, align: TextAlign = TextAlign.Center) =
    BasicText(text, modifier, style = t(size, color, weight, align), maxLines = lines, overflow = TextOverflow.Ellipsis)

/** A button: pressing squeezes it with a springy overshoot and its corners tighten (all inside the draw layer: no relayout, no recomposition per frame). */
@Composable
private fun Pill(modifier: Modifier = Modifier, primary: Boolean = false, enabled: Boolean = true, desc: String? = null, onClick: () -> Unit, content: @Composable () -> Unit) {
    var down by remember { mutableStateOf(false) }
    val p = animateFloatAsState(if (down) 1f else 0f, spring(dampingRatio = 0.5f, stiffness = 520f), label = "press")
    Box(modifier.heightMin().graphicsLayer { val v = p.value; scaleX = 1f - 0.07f * v; scaleY = 1f - 0.07f * v; alpha = if (enabled) 1f else 0.5f; shape = RoundedCornerShape((24f - 10f * v).dp); clip = true }
        .background(if (primary) Clay else Card)
        .semantics { role = Role.Button; if (desc != null) contentDescription = desc }
        .pointerInput(enabled) { detectTapGestures(onPress = { down = true; tryAwaitRelease(); down = false }, onTap = { if (enabled) onClick() }) }
        .padding(horizontal = 12.dp, vertical = 8.dp), contentAlignment = Alignment.Center) { content() }
}

/** Things grow in one after the other: a short spring from below, scaled up, faded in (transform and opacity only). */
@Composable
private fun Modifier.arrive(index: Int = 0): Modifier {
    val a = remember { Animatable(0f) }
    LaunchedEffect(Unit) { delay((index.coerceAtMost(8) * 40).toLong()); a.animateTo(1f, spring(dampingRatio = 0.72f, stiffness = 260f)) }
    return this.graphicsLayer { val v = a.value; alpha = v.coerceIn(0f, 1f); translationY = (1f - v) * 28.dp.toPx(); scaleX = 0.88f + 0.12f * v; scaleY = 0.88f + 0.12f * v }
}
private fun Modifier.heightMin() = this.then(Modifier.heightIn(min = 44.dp))

/** A dot that breathes while Claude works (opacity and size on the GPU only). */
@Composable
private fun Pulse(color: Color, size: Int = 8) {
    val tr = rememberInfiniteTransition(label = "pulse")
    val v by tr.animateFloat(0.35f, 1f, infiniteRepeatable(tween(700, easing = FastOutSlowInEasing), RepeatMode.Reverse), label = "v")
    Box(Modifier.size(size.dp).graphicsLayer { alpha = v; scaleX = 0.75f + 0.25f * v; scaleY = 0.75f + 0.25f * v }.clip(CircleShape).background(color))
}

// ───────────────────────── the round list: full width in the middle, shrinking towards the top and bottom ─────────────────────────

/**
 * The bezel (and crown) scrolls with an eased glide instead of a jump: every click adds to the distance still to go and each frame
 * covers a part of it, so quick turns add up smoothly and the list never skips. A light tick runs along, and a stronger one at an end.
 */
@Composable
private fun Modifier.bezel(state: ScrollableState): Modifier {
    val fr = remember { FocusRequester() }
    val scope = rememberCoroutineScope()
    val view = LocalView.current
    val tick = with(LocalDensity.current) { 36.dp.toPx() }
    val r = remember { object { var left = 0f; var job: Job? = null; var run = 0f; var atEnd = false } }
    LaunchedEffect(Unit) { runCatching { fr.requestFocus() } }
    return this.onRotaryScrollEvent { e ->
        r.left += e.verticalScrollPixels * 1.15f
        if (r.job?.isActive != true) r.job = scope.launch {
            try {
                state.scroll {
                    while (kotlin.math.abs(r.left) > 0.5f) {
                        withFrameNanos { }
                        val step = if (kotlin.math.abs(r.left) < 2f) r.left else r.left * 0.24f
                        val used = scrollBy(step)
                        r.left -= step
                        if (kotlin.math.abs(used) < 0.01f) {   // at the top or bottom: stop and say so once
                            r.left = 0f
                            if (!r.atEnd) { r.atEnd = true; view.performHapticFeedback(if (Build.VERSION.SDK_INT >= 34) HapticFeedbackConstants.SEGMENT_TICK else HapticFeedbackConstants.LONG_PRESS) }
                        } else {
                            r.atEnd = false
                            r.run += kotlin.math.abs(used)
                            if (r.run >= tick) { r.run = 0f; view.performHapticFeedback(if (Build.VERSION.SDK_INT >= 34) HapticFeedbackConstants.SEGMENT_FREQUENT_TICK else HapticFeedbackConstants.CLOCK_TICK) }
                        }
                    }
                }
            } finally { r.left = 0f }
        }
        true
    }.focusRequester(fr).focusable()
}

/** Where a row sits: in the middle it is full size; towards the top or bottom edge it shrinks, fades and moves in, like the watch's own lists. */
private fun Modifier.edge(state: LazyListState, key: Any): Modifier = this.graphicsLayer {
    val info = state.layoutInfo
    val it = info.visibleItemsInfo.firstOrNull { v -> v.key == key } ?: return@graphicsLayer
    val half = (info.viewportEndOffset - info.viewportStartOffset) / 2f
    if (half <= 0f || it.size > half * 1.2f) return@graphicsLayer   // a tall row (a long paragraph) stays as it is
    val d = (it.offset + it.size / 2f - (info.viewportStartOffset + info.viewportEndOffset) / 2f) / half
    val t = ((kotlin.math.abs(d) - 0.45f) / 0.55f).coerceIn(0f, 1f)
    val e = t * t
    val s = 1f - 0.3f * e
    scaleX = s; scaleY = s
    alpha = 1f - 0.7f * e
    translationY = -kotlin.math.sign(d) * it.size * (1f - s) * 0.5f
}

/** The rows of a round list. Every row gets a key, so its place on screen can be found while scrolling. */
private class RoundScope(private val lazy: LazyListScope, private val state: LazyListState) {
    private var n = 0
    fun item(key: Any? = null, content: @Composable () -> Unit) {
        val k = key ?: "row${n}"; n++
        lazy.item(key = k) { Box(Modifier.fillMaxWidth().edge(state, k), contentAlignment = Alignment.Center) { content() } }
    }
}

/** The list of every screen: uses the whole round screen, glides with the bezel and shows where you are on an arc at the right edge. */
@Composable
private fun RoundList(state: LazyListState = rememberLazyListState(), content: RoundScope.() -> Unit) {
    Box(Modifier.fillMaxSize()) {
        LazyColumn(Modifier.fillMaxSize().bezel(state), state = state, contentPadding = PaddingValues(top = 30.dp, bottom = 52.dp),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) { RoundScope(this, state).content() }
        ScrollArc(state)
    }
}

/** A thin arc on the right edge: how long the list is and where you are in it. Shows while scrolling, then fades away. */
@Composable
private fun ScrollArc(state: LazyListState) {
    val moving = state.isScrollInProgress
    var shown by remember { mutableStateOf(false) }
    LaunchedEffect(moving) { if (moving) shown = true else { delay(900); shown = false } }
    val a = animateFloatAsState(if (shown) 1f else 0f, tween(if (shown) 120 else 450, easing = FastOutSlowInEasing), label = "arc")
    Canvas(Modifier.fillMaxSize().graphicsLayer { alpha = a.value }) {
        val info = state.layoutInfo
        val vis = info.visibleItemsInfo
        if (vis.isEmpty() || !(state.canScrollForward || state.canScrollBackward)) return@Canvas
        val avg = vis.sumOf { it.size }.toFloat() / vis.size + 6.dp.toPx()
        val view = (info.viewportEndOffset - info.viewportStartOffset).toFloat()
        val total = (avg * info.totalItemsCount).coerceAtLeast(view)
        val first = vis.first()
        val pos = first.index * avg + (info.viewportStartOffset - first.offset)
        val frac = (view / total).coerceIn(0.12f, 1f)
        val p = if (state.canScrollForward) (pos / (total - view).coerceAtLeast(1f)).coerceIn(0f, 1f) else 1f
        val w = 4.dp.toPx(); val inset = 5.dp.toPx() + w / 2
        val tl = Offset(inset, inset); val sz = Size(size.width - inset * 2, size.height - inset * 2)
        val span = 56f
        drawArc(Card, -span / 2, span, false, tl, sz, style = Stroke(w, cap = StrokeCap.Round))
        val sweep = span * frac
        drawArc(Ink.copy(alpha = 0.85f), -span / 2 + (span - sweep) * p, sweep, false, tl, sz, style = Stroke(w, cap = StrokeCap.Round))
    }
}

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
        val dest = when { servers.isEmpty() -> "none"; open != null -> "chat"; else -> screen }
        val depth = mapOf("none" to 0, "home" to 0, "chats" to 1, "settings" to 1, "chat" to 2)
        AnimatedContent(targetState = dest, modifier = Modifier.fillMaxSize(), label = "screen", transitionSpec = {
            val fwd = depth.getValue(targetState) >= depth.getValue(initialState)
            val sp = spring<IntOffset>(dampingRatio = 0.85f, stiffness = 420f, visibilityThreshold = IntOffset(1, 1))
            val fl = spring<Float>(dampingRatio = 1f, stiffness = 500f)
            (slideInHorizontally(sp) { w -> if (fwd) w / 3 else -w / 3 } + fadeIn(fl) + scaleIn(fl, 0.9f)) togetherWith
                (slideOutHorizontally(sp) { w -> if (fwd) -w / 4 else w / 4 } + fadeOut(fl) + scaleOut(fl, 0.9f))
        }) { d ->
            val o = open
            when {
                d == "none" -> NotPaired()
                d == "chat" && o != null -> ChatScreen(o)
                d == "settings" -> SettingsScreen(servers)
                d == "chats" -> ChatsScreen(servers) { open = it }
                else -> Home(servers, { screen = "chats" }, { screen = "settings" })
            }
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

/** The chats of every server: pinned first (the same pins as on your phone), then the ones that work, then the newest. */
private fun order(l: List<Chat>) = l.sortedWith(compareByDescending<Chat> { it.pinned }.thenByDescending { it.busy }.thenByDescending { it.mtime })

// ───────────────────────── home: what Claude does, at a glance ─────────────────────────

@Composable
private fun Home(servers: List<Server>, onChats: () -> Unit, onSettings: () -> Unit) {
    val c = LocalContext.current
    val first = remember(servers) { servers.mapNotNull { Api.cached(it) } }   // the last numbers show at once, the fresh ones replace them
    var usage by remember { mutableStateOf(first.firstNotNullOfOrNull { it.usage }) }
    var working by remember { mutableStateOf(first.sumOf { h -> h.chats.count { it.busy } }) }
    var err by remember { mutableStateOf("") }
    var asks by remember { mutableStateOf<List<Ask>>(emptyList()) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(servers) {
        while (true) {
            val rs = coroutineScope { servers.map { s -> async(Dispatchers.IO) { runCatching { Api.home(s) } } }.awaitAll() }
            val ok = rs.mapNotNull { it.getOrNull() }
            if (ok.isNotEmpty()) {
                val u = ok.firstNotNullOfOrNull { it.usage } ?: usage; val w = ok.sumOf { h -> h.chats.count { it.busy } }; val a = ok.flatMap { it.asks }
                val changed = u?.session?.toInt() != usage?.session?.toInt() || w != working || a.size != asks.size
                usage = u; working = w; asks = a; err = ""
                if (changed) Complications.refresh(c)   // the face shows the same at once (it reuses this fetch, no extra message)
            }
            else err = friendly(rs.first().exceptionOrNull(), servers.first().name)
            pause(if (working > 0 || asks.isNotEmpty()) 4_000 else 10_000)
        }
    }
    val p = ((usage?.session ?: 0.0) / 100.0).toFloat().coerceIn(0f, 1f)
    val shown by animateFloatAsState(p, spring(dampingRatio = 0.8f, stiffness = 90f), label = "ring")
    val num by animateFloatAsState((usage?.session ?: 0.0).toFloat(), spring(dampingRatio = 1f, stiffness = 120f), label = "num")
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
            Label(if (usage?.session == null) "–" else "${num.toInt()}%", 38, Ink, FontWeight.SemiBold, modifier = Modifier.arrive(0))
            val reset = usage?.sessionReset?.takeIf { it > System.currentTimeMillis() }?.let { java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(java.util.Date(it)) }
            Label(if (reset != null) "resets $reset" else "session", 12, Muted, modifier = Modifier.arrive(1))
            Label("week ${pct(usage?.week)}", 12, Muted, modifier = Modifier.arrive(2))
            Spacer(Modifier.height(8.dp))
            if (err.isNotEmpty()) Label(err, 11, Clay, lines = 2)
            else Row(Modifier.arrive(3), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (working > 0) Pulse(Clay)
                Label(if (working > 0) "$working working" else "all quiet", 12, if (working > 0) Clay else Muted)
            }
            Spacer(Modifier.height(8.dp))
            Row(Modifier.arrive(4), horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
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
    var chats by remember { mutableStateOf<List<Chat>?>(servers.mapNotNull { Api.cached(it) }.takeIf { it.isNotEmpty() }?.flatMap { it.chats }?.let { order(it).take(16) }) }
    LaunchedEffect(servers) {
        while (true) {
            val rs = coroutineScope { servers.map { s -> async(Dispatchers.IO) { runCatching { Api.home(s) } } }.awaitAll() }
            val ok = rs.mapNotNull { it.getOrNull() }
            if (ok.isNotEmpty() || chats == null) chats = order(ok.flatMap { it.chats }).take(16)
            pause(if (chats.orEmpty().any { it.busy }) 6_000 else 15_000)
        }
    }
    RoundList {
        item("head") { Label("Chats", 15, Ink, FontWeight.SemiBold) }
        val l = chats
        when {
            l == null -> item("wait") { Label("Loading…", 12, Muted) }
            l.isEmpty() -> item("none") { Label("No chats yet", 12, Muted) }
            else -> {
                val pinned = l.filter { it.pinned }; val rest = l.filter { !it.pinned }
                if (pinned.isNotEmpty()) { item("hp") { Label("Pinned", 11, Clay, FontWeight.SemiBold) }; pinned.forEach { ch -> item(ch.server.base + ch.id) { ChatRow(ch, onOpen) } } }
                if (pinned.isNotEmpty() && rest.isNotEmpty()) item("hr") { Label("Recent", 11, Muted, FontWeight.SemiBold) }
                rest.forEach { ch -> item(ch.server.base + ch.id) { ChatRow(ch, onOpen) } }
            }
        }
    }
}

@Composable
private fun ChatRow(ch: Chat, onOpen: (Chat) -> Unit) {
    Pill(Modifier.fillMaxWidth(0.9f), onClick = { onOpen(ch) }) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (ch.busy) Pulse(Clay) else if (ch.pinned) Box(Modifier.size(6.dp).clip(CircleShape).background(Clay))
            Column(Modifier.weight(1f)) {
                Label(ch.title, 13, Ink, FontWeight.Medium, lines = 2, align = TextAlign.Start)
                Label(if (ch.busy) "working" else ago(ch.mtime), 11, if (ch.busy) Clay else Muted, lines = 1, align = TextAlign.Start)
            }
        }
    }
}

// ───────────────────────── one chat: a card with what Claude does now, then what it said ─────────────────────────

/** A step of Claude in a few words: "Read /a/b/Api.kt" becomes "Reading Api.kt". */
private fun pretty(s: String): String {
    val sp = s.indexOf(' '); val tool = if (sp < 0) s else s.substring(0, sp); val arg = if (sp < 0) "" else s.substring(sp + 1).trim()
    val file = arg.trimEnd('/').substringAfterLast('/').ifBlank { arg }
    return when (tool) {
        "Read" -> "Reading $file"; "Edit", "MultiEdit", "NotebookEdit" -> "Editing $file"; "Write" -> "Writing $file"
        "Bash" -> "Running ${arg.take(48)}"; "Grep" -> "Searching for $arg"; "Glob" -> "Looking for files"; "LS" -> "Looking in $file"
        "WebFetch" -> "Reading ${arg.substringAfter("://").substringBefore('/')}"; "WebSearch" -> "Searching the web"
        "Task", "Agent" -> "Asking a helper"; "TodoWrite" -> "Planning the next steps"
        else -> if (tool.startsWith("mcp__")) "Using ${tool.split("__").getOrNull(1) ?: "a tool"}" else s
    }.trim()
}

/** Claude's answer in pieces the size of a screen, so it reads like the watch's own texts and every piece moves with the list. */
private fun pieces(text: String): List<String> {
    val out = ArrayList<String>()
    for (para in plain(text).take(3500).split(Regex("\n\\s*\n")).map { it.trim() }.filter { it.isNotEmpty() }) {
        var rest = para
        while (rest.length > 260) {
            val cut = rest.lastIndexOf(". ", 240).takeIf { it > 80 }?.plus(1) ?: rest.lastIndexOf(' ', 240).takeIf { it > 80 } ?: 240
            out.add(rest.substring(0, cut).trim()); rest = rest.substring(cut).trim()
        }
        if (rest.isNotEmpty()) out.add(rest)
    }
    return out
}

@Composable
private fun ChatScreen(ch: Chat) {
    val scope = rememberCoroutineScope()
    var turn by remember { mutableStateOf(turns[ch.server.base + ch.id]) }
    var busy by remember { mutableStateOf(ch.busy) }
    var note by remember { mutableStateOf("") }
    var tick by remember { mutableStateOf(0) }
    LaunchedEffect(ch.id, tick) {
        while (true) {
            runCatching { coroutineScope { val t = async(Dispatchers.IO) { Api.lastTurn(ch.server, ch.id) }; val b = async(Dispatchers.IO) { runCatching { Api.chats(ch.server).firstOrNull { it.id == ch.id }?.busy }.getOrNull() }; t.await() to b.await() } }
                .onSuccess { (tn, b) -> turn = tn; turns[ch.server.base + ch.id] = tn; busy = b ?: false; note = "" }.onFailure { note = friendly(it, ch.server.name) }
            pause(if (busy) 2_500 else 15_000)
        }
    }
    val send: (String) -> Unit = { q -> scope.launch { note = "Sending…"; withContext(Dispatchers.IO) { runCatching { Api.send(ch.server, ch.id, q) } }.onSuccess { busy = true; note = ""; turn = Turn(q, ""); delay(1500); tick++ }.onFailure { note = "Not sent. " + friendly(it, ch.server.name) } } }
    val speak = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r ->
        val said = r.data?.getStringArrayListExtra(RecognizerIntent.EXTRA_RESULTS)?.firstOrNull()?.trim().orEmpty()
        if (said.isNotEmpty()) send(said)
    }
    val tn = turn
    RoundList {
        item("title") { Label(ch.title, 12, Muted, FontWeight.SemiBold, lines = 2, modifier = Modifier.fillMaxWidth(0.7f)) }
        item("card") { NowCard(tn, busy) }
        val plan = tn?.plan.orEmpty()
        if (plan.isNotEmpty()) {
            item("plan") { Label("Plan · ${plan.count { it.status == "completed" }} of ${plan.size} done", 11, Muted, FontWeight.SemiBold, modifier = Modifier.fillMaxWidth(0.8f)) }
            plan.forEachIndexed { i, st -> item("st$i") { StepRow(st, busy) } }
        }
        if (tn != null && !busy && tn.answer.isNotBlank()) pieces(tn.answer).forEachIndexed { i, p ->
            item("p$i") { Label(p, 14, Ink, align = TextAlign.Start, modifier = Modifier.fillMaxWidth(0.84f)) }
        }
        if (note.isNotEmpty()) item("note") { Label(note, 11, Clay, modifier = Modifier.fillMaxWidth(0.8f)) }
        item("act") {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Pill(Modifier.width(if (busy) 80.dp else 128.dp), primary = true, enabled = !busy, desc = "Reply by voice", onClick = {
                    runCatching { speak.launch(Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM).putExtra(RecognizerIntent.EXTRA_PROMPT, "Say it to Claude")) }.onFailure { note = "No voice input on this watch" }
                }) { Label("Reply", 13, Pal.cur.onClay, FontWeight.SemiBold) }
                if (busy) Pill(Modifier.width(80.dp), desc = "Stop", onClick = { scope.launch { withContext(Dispatchers.IO) { runCatching { Api.stop(ch.server, ch.id) } }; delay(800); tick++ } }) { Label("Stop", 13, Ink) }
            }
        }
        if (!busy && tn?.answer?.isNotBlank() == true) listOf("Yes, go ahead", "Continue", "Explain shorter").forEach { q ->
            item("q$q") { Pill(Modifier.fillMaxWidth(0.8f), onClick = { send(q) }) { Label(q, 12, Ink, lines = 1) } }
        }
    }
}

/** One step of the plan: a check when done, a turning ring while Claude works on it, an empty ring when it is still to come. */
@Composable
private fun StepRow(st: Step, busy: Boolean) {
    val done = st.status == "completed"; val now = st.status == "in_progress"
    Row(Modifier.fillMaxWidth(0.84f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Box(Modifier.size(20.dp), contentAlignment = Alignment.Center) {
            when {
                done -> Canvas(Modifier.size(20.dp)) {
                    drawCircle(Clay)
                    val w = 2.2.dp.toPx(); val p = androidx.compose.ui.graphics.Path().apply { moveTo(size.width * 0.28f, size.height * 0.52f); lineTo(size.width * 0.44f, size.height * 0.67f); lineTo(size.width * 0.73f, size.height * 0.36f) }
                    drawPath(p, Pal.cur.onClay, style = Stroke(w, cap = StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
                }
                now && busy -> Spinner(Clay)
                else -> Canvas(Modifier.size(20.dp)) { val w = 2.dp.toPx(); drawCircle(if (now) Clay else Muted, radius = size.minDimension / 2 - w / 2, style = Stroke(w)) }
            }
        }
        Label(st.text, 13, if (done) Muted else Ink, if (now) FontWeight.Medium else FontWeight.Normal, lines = 3, align = TextAlign.Start, modifier = Modifier.weight(1f))
    }
}

/** A turning ring: one arc rotated on the GPU, nothing laid out again. */
@Composable
private fun Spinner(color: Color) {
    val tr = rememberInfiniteTransition(label = "spin")
    val a by tr.animateFloat(0f, 360f, infiniteRepeatable(tween(900, easing = androidx.compose.animation.core.LinearEasing)), label = "a")
    Canvas(Modifier.size(20.dp).graphicsLayer { rotationZ = a }) {
        val w = 2.4.dp.toPx()
        drawCircle(color.copy(alpha = 0.25f), radius = size.minDimension / 2 - w / 2, style = Stroke(w))
        drawArc(color, 0f, 100f, false, Offset(w / 2, w / 2), Size(size.width - w, size.height - w), style = Stroke(w, cap = StrokeCap.Round))
    }
}

/** The card at the top of a chat: while Claude works, the step it is on now (sliding in as it changes) and the ones before; when it is done, when that was and what you asked. */
@Composable
private fun NowCard(tn: Turn?, busy: Boolean) {
    Column(Modifier.fillMaxWidth(0.88f).clip(RoundedCornerShape(22.dp)).background(Card).animateContentSize(spring(dampingRatio = 0.85f, stiffness = 380f)).padding(horizontal = 16.dp, vertical = 12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            if (busy) Pulse(Clay) else Box(Modifier.size(8.dp).clip(CircleShape).background(Muted))
            Label(if (busy) "Working" else if (tn == null) "Loading" else "Done", 12, if (busy) Clay else Muted, FontWeight.SemiBold)
            if (tn != null && tn.ts > 0) Label("· " + ago(tn.ts), 11, Muted, lines = 1)
        }
        Spacer(Modifier.height(6.dp))
        if (tn == null) return@Column
        if (busy) {
            val now = when { tn.now == "tool" && tn.steps.isNotEmpty() -> pretty(tn.steps.last()); tn.now == "assistant" && tn.answer.isNotBlank() -> plain(tn.answer).lineSequence().firstOrNull { it.isNotBlank() }.orEmpty().take(110); else -> "Thinking…" }
            AnimatedContent(now, label = "step", transitionSpec = {
                (slideInVertically(spring(dampingRatio = 0.8f, stiffness = 380f)) { it / 2 } + fadeIn(tween(180))) togetherWith (slideOutVertically(tween(160)) { -it / 2 } + fadeOut(tween(120)))
            }) { s -> Label(s, 14, Ink, FontWeight.Medium, lines = 3, align = TextAlign.Start, modifier = Modifier.fillMaxWidth()) }
            val before = (if (tn.now == "tool") tn.steps.dropLast(1) else tn.steps).takeLast(2)
            if (before.isNotEmpty()) { Spacer(Modifier.height(6.dp)); before.reversed().forEach { Label("✓ " + pretty(it), 11, Muted, lines = 1, align = TextAlign.Start, modifier = Modifier.fillMaxWidth()) } }
        } else if (tn.asked.isNotBlank()) {
            Label("You asked", 10, Muted, align = TextAlign.Start, modifier = Modifier.fillMaxWidth())
            Label(plain(tn.asked).take(140), 12, Ink, lines = 3, align = TextAlign.Start, modifier = Modifier.fillMaxWidth())
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
    RoundList {
        item("head") { Label("Settings", 15, Ink, FontWeight.SemiBold) }
        if (Update.faceBehind(c)) item("fb") { Label("Your watch face is from an older Hearth: update it below to get the theme and the new options.", 11, Clay, modifier = Modifier.fillMaxWidth(0.72f)) }
        item("th") { Label("Theme · also on the Hearth watch face", 11, Muted, FontWeight.SemiBold, modifier = Modifier.fillMaxWidth(0.7f)) }
        fun row(id: String, label: String, sub: String?, dot: Color?) = item(id) {
            val on = Pal.choice == id
            Pill(Modifier.fillMaxWidth(0.9f), primary = on, onClick = { Pal.pick(c, id) }) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Box(Modifier.size(14.dp).clip(CircleShape).background(dot ?: if (on) Pal.cur.onClay else Clay))
                    Column(Modifier.weight(1f)) {
                        Label(label, 13, if (on) Pal.cur.onClay else Ink, FontWeight.Medium, lines = 1, align = TextAlign.Start)
                        if (sub != null) Label(sub, 10, if (on) Pal.cur.onClay else Muted, lines = 1, align = TextAlign.Start)
                    }
                }
            }
        }
        row("watch", "My watch", "Hearth's own look", null)
        row("pc", "My computer", if (Pal.pcName.isNotBlank()) Pal.pcName else "not shared yet", null)
        Pal.PRESETS.forEach { row(it.name, it.name, null, it.clay) }
        item("uh") { Label("Updates", 11, Muted, FontWeight.SemiBold) }
        item("uv") { Label("Hearth ${Update.current(c)}" + (if (Update.status.isNotBlank()) "\n" + Update.status else ""), 11, Muted, lines = 3, modifier = Modifier.fillMaxWidth(0.7f)) }
        item("uc") { Pill(Modifier.fillMaxWidth(0.9f), enabled = !Update.busy, onClick = { if (!Update.busy) scope.launch { Update.check(c) } }) { Label("Check for updates", 13, Ink, lines = 1) } }
        item("uf") {
            val behind = Update.faceBehind(c); val has = Update.faceVersion(c) != null
            Pill(Modifier.fillMaxWidth(0.9f), primary = behind, enabled = !Update.busy, onClick = { if (!Update.busy) scope.launch { Update.install(c, face = true) } }) {
                Label(if (behind) "Update the watch face" else if (has) "Reinstall the watch face" else "Install the watch face", 13, if (behind) Pal.cur.onClay else Ink, lines = 1)
            }
        }
        if (Update.ready(c)) item("uu") { Pill(Modifier.fillMaxWidth(0.9f), primary = true, enabled = !Update.busy, onClick = { if (!Update.busy) scope.launch { Update.install(c) } }) { Label("Update to ${Update.latest}", 13, Pal.cur.onClay, FontWeight.SemiBold, lines = 1) } }
    }
}
