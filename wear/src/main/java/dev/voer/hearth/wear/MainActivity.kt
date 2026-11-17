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
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.wear.compose.foundation.lazy.ScalingLazyColumn
import androidx.wear.compose.foundation.lazy.rememberScalingLazyListState
import androidx.wear.compose.material.Chip
import androidx.wear.compose.material.ChipDefaults
import androidx.wear.compose.material.CircularProgressIndicator
import androidx.wear.compose.material.Colors
import androidx.wear.compose.material.MaterialTheme
import androidx.wear.compose.material.PositionIndicator
import androidx.wear.compose.material.Scaffold
import androidx.wear.compose.material.Text
import androidx.wear.compose.material.TimeText
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

private val Clay get() = Pal.cur.clay
private val Ink get() = Pal.cur.ink
private val Muted get() = Pal.cur.muted
private val Card get() = Pal.cur.card

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
        if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE == 0) return
        i?.getStringExtra("machines")?.let { Servers.save(this, it) }
    }
}

private suspend fun pause(ms: Long) { delay(ms); while (!Visible.now) delay(500) }

@Composable
fun WatchApp() {
    val c = LocalContext.current
    MaterialTheme(colors = Colors(primary = Clay, onPrimary = Pal.cur.onClay, surface = Card, onSurface = Ink, onSurfaceVariant = Muted, background = Pal.cur.bg, onBackground = Ink)) {
        val rev = Servers.rev
        val servers = remember(rev) { Servers.all(c) }
        LaunchedEffect(Unit) { Servers.fetchFromPhone(c) }
        var open by remember { mutableStateOf<Chat?>(null) }
        var settings by remember { mutableStateOf(false) }
        BackHandler(open != null || settings) { if (open != null) open = null else settings = false }
        val o = open
        when {
            settings -> SettingsScreen(servers)
            servers.isEmpty() -> NotPaired()
            o == null -> Home(servers, { settings = true }) { open = it }
            else -> ChatScreen(o)
        }
    }
}

@Composable
private fun NotPaired() {
    Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        Text("Open Hearth on your phone once. Your servers come over by themselves.", textAlign = TextAlign.Center, fontSize = 14.sp, color = Ink)
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

@Composable
private fun Home(servers: List<Server>, onSettings: () -> Unit, onOpen: (Chat) -> Unit) {
    var usage by remember { mutableStateOf<Usage?>(null) }
    var chats by remember { mutableStateOf<List<Chat>?>(null) }
    var err by remember { mutableStateOf("") }
    LaunchedEffect(servers) {
        while (true) {
            val r = withContext(Dispatchers.IO) {
                val u = servers.firstNotNullOfOrNull { s -> runCatching { Api.usage(s) }.getOrNull() }
                val l = servers.map { s -> runCatching { Api.chats(s) } }
                Triple(u, l.flatMap { it.getOrDefault(emptyList()) }.sortedByDescending { it.mtime }.take(8), (if (l.all { it.isFailure }) friendly(l.first().exceptionOrNull(), servers.first().name) else null))
            }
            usage = r.first ?: usage; chats = r.second; err = r.third ?: ""
            pause(if (r.second.any { it.busy }) 6_000 else 20_000)
        }
    }
    val state = rememberScalingLazyListState()
    Scaffold(timeText = { TimeText() }, positionIndicator = { PositionIndicator(scalingLazyListState = state) }) {
        val s = usage?.session
        if (s != null) CircularProgressIndicator(progress = (s / 100).toFloat().coerceIn(0f, 1f), modifier = Modifier.fillMaxSize().padding(2.dp), startAngle = 300f, endAngle = 240f,
            indicatorColor = if (s >= 90) Color(0xFFE5484D) else Clay, trackColor = Color(0xFF34322E), strokeWidth = 4.dp)
        ScalingLazyColumn(Modifier.fillMaxSize(), state = state, horizontalAlignment = Alignment.CenterHorizontally) {
            item { Text(pct(usage?.session), fontSize = 30.sp, fontWeight = FontWeight.SemiBold, color = Ink) }
            item {
                val reset = usage?.sessionReset?.takeIf { it > System.currentTimeMillis() }?.let { java.text.DateFormat.getTimeInstance(java.text.DateFormat.SHORT).format(java.util.Date(it)) }
                Text("session" + (reset?.let { " · resets $it" } ?: "") + "\nweek " + pct(usage?.week), fontSize = 12.sp, color = Muted, textAlign = TextAlign.Center)
            }
            item { Spacer(Modifier.height(6.dp)) }
            if (err.isNotEmpty()) item { Text(err, fontSize = 12.sp, color = Clay, textAlign = TextAlign.Center) }
            else if (Phone.via == false) item { Text("Phone not in reach · using the watch's own connection", fontSize = 10.sp, color = Muted, textAlign = TextAlign.Center) }
            val l = chats
            if (l == null) item { Text("Loading…", fontSize = 12.sp, color = Muted) }
            else if (l.isEmpty() && err.isEmpty()) item { Text("No chats yet", fontSize = 12.sp, color = Muted) }
            else l.forEach { ch ->
                item(key = ch.server.base + ch.id) {
                    Chip(onClick = { onOpen(ch) }, modifier = Modifier.fillMaxWidth(),
                        label = { Text(ch.title, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                        secondaryLabel = { Text(if (ch.busy) "Working…" else ago(ch.mtime), color = if (ch.busy) Clay else Muted) },
                        colors = ChipDefaults.secondaryChipColors())
                }
            }
            item { Chip(onClick = onSettings, modifier = Modifier.fillMaxWidth(), label = { Text("Settings") }, secondaryLabel = { Text("Theme, updates") }, colors = ChipDefaults.secondaryChipColors()) }
        }
    }
}

@Composable
private fun SettingsScreen(servers: List<Server>) {
    val c = LocalContext.current; val scope = rememberCoroutineScope()
    val state = rememberScalingLazyListState()
    LaunchedEffect(servers) {   // the computer's own look, when one of the servers has it
        val got = withContext(Dispatchers.IO) { servers.firstNotNullOfOrNull { s -> runCatching { Api.theme(s) }.getOrNull() } }
        if (got != null) Pal.savePc(c, got)
    }
    Scaffold(timeText = { TimeText() }, positionIndicator = { PositionIndicator(scalingLazyListState = state) }) {
        ScalingLazyColumn(Modifier.fillMaxSize(), state = state, horizontalAlignment = Alignment.CenterHorizontally) {
            item { Text("Theme", fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = Ink) }
            fun row(id: String, label: String, sub: String?) = item(key = id) {
                Chip(onClick = { Pal.pick(c, id) }, modifier = Modifier.fillMaxWidth(), label = { Text(label) },
                    secondaryLabel = sub?.let { { Text(it) } }, icon = { if (Pal.choice == id) Text("✓", color = Clay) },
                    colors = if (Pal.choice == id) ChipDefaults.primaryChipColors() else ChipDefaults.secondaryChipColors())
            }
            row("watch", "My watch", "Hearth's own look")
            row("pc", "My computer", if (Pal.pcName.isNotBlank()) Pal.pcName else "Not shared by your computer yet")
            Pal.PRESETS.forEach { row(it.name, it.name, null) }
            item { Spacer(Modifier.height(8.dp)) }
            item { Text("Updates", fontSize = 14.sp, fontWeight = FontWeight.SemiBold, color = Ink) }
            item { Text("Hearth ${Update.current(c)}" + (if (Update.status.isNotBlank()) "\n" + Update.status else ""), fontSize = 12.sp, color = Muted, textAlign = TextAlign.Center) }
            item { Chip(onClick = { if (!Update.busy) scope.launch { Update.check(c) } }, modifier = Modifier.fillMaxWidth(), enabled = !Update.busy, label = { Text("Check for updates") }, colors = ChipDefaults.secondaryChipColors()) }
            if (Update.ready(c)) item { Chip(onClick = { if (!Update.busy) scope.launch { Update.install(c) } }, modifier = Modifier.fillMaxWidth(), enabled = !Update.busy, label = { Text("Update to ${Update.latest}") }, colors = ChipDefaults.primaryChipColors()) }
        }
    }
}

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
                .onSuccess { (t, b) -> turn = t; busy = b ?: false; note = "" }.onFailure { note = friendly(it, ch.server.name) }
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
    val state = rememberScalingLazyListState()
    Scaffold(timeText = { TimeText() }, positionIndicator = { PositionIndicator(scalingLazyListState = state) }) {
        ScalingLazyColumn(Modifier.fillMaxSize(), state = state, horizontalAlignment = Alignment.CenterHorizontally) {
            item { Text(ch.title, fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Ink, maxLines = 2, overflow = TextOverflow.Ellipsis, textAlign = TextAlign.Center) }
            val t = turn
            if (t == null) item { Text("Loading…", fontSize = 12.sp, color = Muted) }
            else {
                if (t.asked.isNotBlank()) item { Text(plain(t.asked).take(160), fontSize = 12.sp, color = Muted, textAlign = TextAlign.Center, maxLines = 3, overflow = TextOverflow.Ellipsis) }
                if (busy) item { Text("Claude is working…", fontSize = 14.sp, color = Clay) }
                else if (t.answer.isNotBlank()) plain(t.answer).take(1500).split("\n\n").forEach { para -> item { Text(para, fontSize = 14.sp, color = Ink, modifier = Modifier.fillMaxWidth().padding(horizontal = 6.dp)) } }
            }
            if (note.isNotEmpty()) item { Text(note, fontSize = 12.sp, color = Clay, textAlign = TextAlign.Center) }
            item { Spacer(Modifier.height(4.dp)) }
            item {
                Chip(onClick = {
                    runCatching { speak.launch(Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH).putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM).putExtra(RecognizerIntent.EXTRA_PROMPT, "Say it to Claude")) }.onFailure { note = "No voice input on this watch" }
                }, modifier = Modifier.fillMaxWidth(), enabled = !busy, label = { Text("Reply") }, secondaryLabel = { Text("By voice") }, colors = ChipDefaults.primaryChipColors())
            }
            if (busy) item {
                Chip(onClick = { scope.launch { withContext(Dispatchers.IO) { runCatching { Api.stop(ch.server, ch.id) } }; delay(800); tick++ } },
                    modifier = Modifier.fillMaxWidth(), label = { Text("Stop") }, colors = ChipDefaults.secondaryChipColors())
            }
        }
    }
}
