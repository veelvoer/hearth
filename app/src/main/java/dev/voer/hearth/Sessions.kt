package dev.voer.hearth

import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.unit.sp
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

@Composable
fun Field(value: String, onChange: (String) -> Unit, hint: String, mono: Boolean = false, number: Boolean = false, singleLine: Boolean = true, modifier: Modifier = Modifier, bare: Boolean = false) {
    val cs = MaterialTheme.colorScheme
    Box(if (bare) modifier.fillMaxWidth().padding(vertical = 10.dp) else modifier.fillMaxWidth().clip(RoundedCornerShape(16.dp)).background(cs.surfaceVariant)
        .border(1.dp, cs.outline, RoundedCornerShape(16.dp)).padding(14.dp)) {
        if (value.isEmpty()) Txt(hint, T.body, muted = true)
        BasicTextField(value, onChange, singleLine = singleLine, maxLines = if (singleLine) 1 else 6, cursorBrush = SolidColor(cs.primary),
            keyboardOptions = KeyboardOptions(keyboardType = if (number) KeyboardType.Number else KeyboardType.Text),
            textStyle = T.body.copy(color = cs.onSurface, fontFamily = if (mono) FontFamily.Monospace else FontFamily.Default),
            modifier = Modifier.fillMaxWidth())
    }
}

private fun short(p: String) = p.trimEnd('/').split('/').takeLast(2).joinToString("/")
private fun folder(p: String) = p.trimEnd('/').substringAfterLast('/').ifEmpty { "Other" }

// ───────────────────────── markdown ─────────────────────────

private fun inline(s: String, code: androidx.compose.ui.graphics.Color, bg: androidx.compose.ui.graphics.Color): AnnotatedString = buildAnnotatedString {
    val re = Regex("(`[^`\\n]+`)|(\\*\\*[^*\\n]+\\*\\*)")
    var last = 0
    for (m in re.findAll(s)) {
        append(s.substring(last, m.range.first))
        val t = m.value
        if (t.startsWith("**")) withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(t.substring(2, t.length - 2)) }
        else withStyle(SpanStyle(fontFamily = FontFamily.Monospace, background = bg, color = code)) { append(t.substring(1, t.length - 1)) }
        last = m.range.last + 1
    }
    append(s.substring(last))
}

@Composable
fun MarkdownText(text: String) {
    val cs = MaterialTheme.colorScheme
    val parts = remember(text) {
        val out = ArrayList<Pair<Boolean, String>>()  // (isCode, text)
        var code = false
        val cur = StringBuilder()
        for (line in text.lines()) {
            if (line.trimStart().startsWith("```")) { if (cur.isNotEmpty() || code) out += code to cur.toString().trimEnd('\n'); cur.clear(); code = !code; continue }
            cur.append(line).append('\n')
        }
        if (cur.isNotEmpty()) out += code to cur.toString().trimEnd('\n')
        out
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        parts.forEach { (isCode, body) ->
            if (isCode) {
                val dk = cs.background.luminance() < .5f
                Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(if (dk) Color(0xFF141312) else Color(0xFF2A2823))
                    .horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 10.dp)) { Txt(body, T.small.copy(fontFamily = FontFamily.Monospace, lineHeight = 18.sp), color = if (dk) Color(0xFFE9E6DD) else Color(0xFFF1EEE6)) }
            } else body.split(Regex("\\n\\s*\\n")).filter { it.isNotBlank() }.forEach { para ->
                val lines = para.lines()
                Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
                    lines.forEach { l ->
                        val t = l.trimEnd()
                        when {
                            t.startsWith("#") -> Txt(t.trimStart('#').trim(), T.h2)
                            Regex("^\\s*([-*+]|\\d+[.)])\\s+.*").matches(t) -> Row {
                                Txt(if (t.trimStart()[0].isDigit()) t.trimStart().substringBefore(' ') + " " else "•  ", T.body, muted = true)
                                Text(inline(t.trimStart().substringAfter(' '), cs.primary, cs.surfaceVariant), style = T.body, color = cs.onSurface)
                            }
                            else -> Text(inline(t, cs.primary, cs.surfaceVariant), style = T.body, color = cs.onSurface)
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ToolCard(name: String, detail: String, result: String?, err: Boolean) {
    var open by remember { mutableStateOf(false) }
    val cs = MaterialTheme.colorScheme
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(cs.surfaceVariant).border(1.dp, cs.outline, RoundedCornerShape(12.dp))
        .clickable { open = !open }.padding(horizontal = 12.dp, vertical = 9.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Txt(name.ifEmpty { "Tool" }, T.small.copy(fontWeight = FontWeight.SemiBold), color = cs.primary)
            Spacer(Modifier.width(10.dp))
            Text(detail, style = T.small.copy(fontFamily = FontFamily.Monospace), color = cs.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            Spacer(Modifier.width(8.dp))
            Txt(if (result == null) "…" else if (err) "✗" else "✓", T.small, color = if (err) cs.error else cs.onSurfaceVariant)
        }
        if (open) {
            if (detail.isNotEmpty()) { Spacer(Modifier.height(6.dp)); Txt(detail, T.small.copy(fontFamily = FontFamily.Monospace)) }
            if (!result.isNullOrEmpty()) { Spacer(Modifier.height(6.dp)); Txt(result.take(1500), T.small.copy(fontFamily = FontFamily.Monospace), muted = true) }
        }
    }
}

// ───────────────────────── navigation ─────────────────────────

private sealed interface View {
    data object Home : View
    data object New : View
    data class Chat(val m: Machine, val s: Sess, val voice: Boolean = false) : View
}

@Composable
fun SessionsScreen() {
    val c = LocalContext.current
    var machines by remember(Ui.machinesRev) { mutableStateOf(Store.machines(c)) }
    var view by remember { mutableStateOf<View>(View.Home) }
    // the shared chats of friends: kept on your own server too, so every device of yours has the same ones
    var guests by remember { mutableStateOf(Shares.guests(c)) }
    var joinOpen by remember { mutableStateOf(false) }
    LaunchedEffect(Ui.machinesRev) { while (true) { machines.firstOrNull { it.secure }?.let { m -> runCatching { withContext(Dispatchers.IO) { Shares.sync(c, m) } } }; val before = guests; guests = Shares.guests(c); if (before.map { it.share + it.token } != guests.map { it.share + it.token }) CallService.sync(c); delay(8_000) } }
    LaunchedEffect(Ui.shareRev) { if (Ui.pendingJoin != null) joinOpen = true }
    if (joinOpen) JoinDialog(Ui.pendingJoin ?: "", { joinOpen = false; Ui.pendingJoin = null }) { g -> joinOpen = false; Ui.pendingJoin = null; guests = Shares.guests(c); view = View.Home; CallService.sync(c) }
    val po = Ui.pendingOpen
    LaunchedEffect(po) {
        if (po != null) {
            (Store.machines(c) + Shares.guests(c)).firstOrNull { it.host == po.host }?.let { view = View.Chat(it, Sess(po.session, po.title, po.cwd, 0, true), po.voice) }
            Ui.pendingOpen = null
        }
    }
    val v = view
    BackHandler(v !is View.Home) { view = View.Home }
    when (v) {
        is View.Home -> Home(machines, guests, { machines = it; Store.saveMachines(c, it); CallService.sync(c) }, { view = View.New }, { m -> view = View.Chat(m, Sess("", "New talk", "", 0, false, kind = "talk")) }, { joinOpen = true }) { m, s -> view = View.Chat(m, s) }
        is View.New -> NewSession(machines, { view = View.Home }) { m, path, kind, asRoot -> view = View.Chat(m, Sess("", if (kind == "talk") "New talk" else "New session", path, 0, false, kind = kind, root = asRoot)) }
        is View.Chat -> Chat(v.m, v.s, v.voice, onClear = { view = View.Chat(v.m, Sess("", "New session", v.s.cwd, 0, false, kind = v.s.kind, root = v.s.root)) }) { view = View.Home }
    }
}

@Composable
private fun Home(machines: List<Machine>, guests: List<Machine>, save: (List<Machine>) -> Unit, newSession: () -> Unit, talk: (Machine) -> Unit, join: () -> Unit, open: (Machine, Sess) -> Unit) {
    val c = LocalContext.current
    val lists = remember { mutableStateMapOf<String, List<Sess>?>() }
    val now by tick()
    val current by rememberUpdatedState(machines)
    LaunchedEffect(Unit) {
        while (true) {  // a computer whose IP address changed is found again by name
            val f = withContext(Dispatchers.IO) { Relay.discover() }
            val moved = current.map { mm -> if (mm.secure) mm else f.firstOrNull { it.name == mm.name && it.host != mm.host }?.let { mm.copy(host = it.host, port = it.port) } ?: mm }
            if (moved != current) save(moved)
            delay(30_000)
        }
    }
    LaunchedEffect(machines, guests) {
        while (true) {
            // every computer is asked at the same time and shows up as soon as it answers; a computer that is off must not hold the others back
            (machines + guests).map { mm -> launch { lists[mm.host + "|" + mm.token.take(8)] = runCatching { withContext(Dispatchers.IO) { Relay.sessions(mm) } }.getOrNull() } }.forEach { it.join() }
            delay(6_000)
        }
    }
    var query by remember { mutableStateOf("") }
    var onlyWorking by remember { mutableStateOf(false) }
    var onlyShared by remember { mutableStateOf(false) }   // shared chats are kept apart: not in the normal list, but under their own chip
    Box(Modifier.fillMaxSize()) {
    Screen {
        Header("Chats") {
            if (machines.isNotEmpty()) Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(40.dp).clip(RoundedCornerShape(50)).background(MaterialTheme.colorScheme.surfaceVariant).border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(50))
                    .clickable { talk(machines.firstOrNull { it.secure } ?: machines.first()) }, contentAlignment = Alignment.Center) { Glyph("mic", 18.dp) }
                Box(Modifier.size(44.dp).tourTarget("new").clip(RoundedCornerShape(50)).background(MaterialTheme.colorScheme.primary).clickable { newSession() }, contentAlignment = Alignment.Center) {
                    Txt("+", T.title.copy(fontSize = 26.sp), color = MaterialTheme.colorScheme.onPrimary)
                }
            }
        }
        if (machines.isEmpty()) {
            Card {
                Txt("Connect a computer", T.h2)
                Spacer(Modifier.height(6.dp))
                Txt("Put the phone on the same Wi-Fi as your computer and open Hearth there. Or use the address and code of your server.", T.small, muted = true)
                Spacer(Modifier.height(10.dp))
                FindComputers { list -> Store.setCallMe(c, true); save(list) }
                Spacer(Modifier.height(10.dp))
                AddComputerInline(machines) { save(it); if (machines.isEmpty()) Store.setCallMe(c, true) }
            }
            return@Screen
        }
        Field(query, { query = it }, "Search chats")
        ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Chip("All", !onlyWorking && !onlyShared) { onlyWorking = false; onlyShared = false }; Chip("Working", onlyWorking) { onlyWorking = true; onlyShared = false }; Chip("Shared", onlyShared) { onlyShared = true; onlyWorking = false }; Chip("Join a shared chat ›", false) { join() } }
        fun key(mm: Machine) = mm.host + "|" + mm.token.take(8)
        val offline = machines.filter { lists.containsKey(key(it)) && lists[key(it)] == null }
        if (offline.isNotEmpty()) Txt(offline.joinToString { it.name } + " can't be reached right now. It will reconnect on its own.", T.small, muted = true)
        // the server holds every chat, so its list is the list (same as on the desktop app); other computers only fill in what it lacks
        val hub = machines.firstOrNull { it.secure && lists[key(it)] != null }
        val all = (machines + guests).flatMap { mm -> (lists[key(mm)] ?: emptyList()).map { mm to it } }
            .groupBy { (mm, s) -> if (mm.guest) mm.share + ":" + s.id else s.id }.values.map { g ->
                g.firstOrNull { (mm, _) -> mm === hub } ?: g.reduce { a, b -> if (a.second.running != b.second.running) (if (a.second.running) a else b) else if (b.second.mtime > a.second.mtime + 2500) b else a }
            }.sortedByDescending { (_, s) -> if (s.running) Long.MAX_VALUE else s.mtime }.take(60)
            .filter { (mm, s) -> (if (onlyShared) (mm.guest || s.shared >= 0) else onlyWorking || !(mm.guest || s.shared >= 0)) && (!onlyWorking || s.running || s.busy) && (query.isBlank() || (s.title + " " + folder(s.cwd)).contains(query.trim(), ignoreCase = true)) }
        if (all.isEmpty() && offline.size < machines.size && lists.size == machines.size + guests.size) Txt("No sessions yet. Tap New session to start one.", T.small, muted = true)
        all.forEachIndexed { i, (mm, s) -> Box(if (i == 0) Modifier.tourTarget("list") else Modifier) { SessionCard(s, if (mm.guest) "shared by " + mm.label else if (s.shared >= 0) (if (s.shared > 0) "shared with ${s.shared}" else "shared") else s.elsewhere.ifBlank { null }?.let { "only on $it" }, now) { open(mm, s) } } }
    }
    }
}

@Composable
private fun SessionCard(s: Sess, machine: String?, now: Long, onClick: () -> Unit) {
    val cs = MaterialTheme.colorScheme
    val working = s.running || s.busy
    val dark = cs.background.luminance() < .5f
    val shape = RoundedCornerShape(20.dp)
    Row(Modifier.fillMaxWidth().pressable(onClick).panel(shape).padding(14.dp), verticalAlignment = Alignment.CenterVertically) {
        val name = if (s.kind == "talk") "Talk" else folder(s.cwd)
        Box(Modifier.size(44.dp).clip(RoundedCornerShape(14.dp)).background(if (dark) Color(0xFF2E2A26) else Color(0xFFE9E2D2)), contentAlignment = Alignment.Center) {
            Txt(name.take(1).uppercase(), T.title.copy(fontSize = 20.sp), color = cs.primary)
        }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f)) {
            Text(s.title, style = T.body.copy(fontSize = 16.sp, fontWeight = FontWeight.SemiBold), color = cs.onSurface, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (working) { Box(Modifier.size(6.dp).clip(RoundedCornerShape(50)).background(cs.primary)); Spacer(Modifier.width(6.dp)) }
                Txt(listOfNotNull(if (working) "Working" else null, name, machine, if (working) null else dur(now - s.mtime)).joinToString(" · "),
                    T.small.copy(fontSize = 12.sp, fontWeight = if (working) FontWeight.SemiBold else FontWeight.Normal), muted = !working, color = if (working) cs.primary else Color.Unspecified)
            }
        }
    }
}

@Composable
internal fun AddComputer(suggest: Machine?, done: (Machine?) -> Unit) {
    val scope = rememberCoroutineScope()
    var name by remember { mutableStateOf("") }
    var host by remember { mutableStateOf(suggest?.host ?: "") }
    var tok by remember { mutableStateOf("") }
    var err by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    Card {
        Label("Add a computer or server")
        Spacer(Modifier.height(6.dp))
        Txt("Paste the pairing link, or enter the address and the token or 6-digit code.", T.small, muted = true)
        Spacer(Modifier.height(10.dp)); Field(host, { host = it.trim() }, "Pairing link, https://… or 192.168.1.20")
        Spacer(Modifier.height(8.dp)); Field(tok, { tok = it.trim() }, "Token or 6-digit code (not needed with a link)", mono = true)
        Spacer(Modifier.height(8.dp)); Field(name, { name = it }, "Name (optional)")
        err?.let { Spacer(Modifier.height(6.dp)); Txt(it, T.small, color = MaterialTheme.colorScheme.primary) }
        Spacer(Modifier.height(12.dp))
        Button(if (busy) "Connecting…" else "Connect", Modifier.fillMaxWidth()) {
            if (busy) return@Button
            busy = true; err = null
            scope.launch {
                val r = withContext(Dispatchers.IO) {
                    runCatching {
                        val m = parseMachine(host, name, tok) ?: throw ApiException(0, "Enter an address first")
                        if (!host.startsWith("hearth://") && !host.startsWith("hearth://") && Regex("\\d{6}").matches(tok)) Relay.pair(m, tok).copy(name = name.ifBlank { m.host })
                        else { Relay.sessions(m); m }   // proves the address and token work before saving
                    }
                }
                r.onSuccess { done(it) }.onFailure { err = if (it is ApiException && it.code == 401) "That token isn't right." else it.message ?: "Couldn't reach it" }
                busy = false
            }
        }
        TextAction("Cancel") { done(null) }
    }
}

@Composable
private fun AddComputerInline(machines: List<Machine>, onSaved: (List<Machine>) -> Unit) {
    val c = LocalContext.current
    AddComputer(null) { r -> if (r != null) { onSaved(machines.filter { it.host != r.host } + r); Toast.makeText(c, "Connected to ${r.name}", Toast.LENGTH_SHORT).show() } }
}

/** Everything about which computers and servers this phone talks to lives here, not on the sessions screen. */
@Composable
fun ComputersCard() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var machines by remember(Ui.machinesRev) { mutableStateOf(Store.machines(c)) }
    var adding by remember { mutableStateOf(false) }
    var manual by remember { mutableStateOf(false) }
    fun commit(l: List<Machine>) { machines = l; Store.saveMachines(c, l); Ui.machinesRev++; CallService.sync(c) }
    Card(Modifier.tourTarget("computers")) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Head("server", "Computers and servers", "The computers and servers this phone talks to. Chats run on whichever you pick. Add one by tapping Connect next to a computer on your Wi-Fi, or with a server address and 6-digit code.", Modifier.weight(1f))
        }
        machines.forEach { mm ->
            Spacer(Modifier.height(10.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) { Txt(mm.name, T.body); Txt(if (mm.secure) "Server · ${mm.host}" else "Computer · ${mm.host}", T.small, muted = true) }
                TextAction("Remove") { commit(machines - mm) }
            }
        }
        Spacer(Modifier.height(8.dp))
        if (adding) {
            FindComputers { list ->
                if (machines.isEmpty()) Store.setCallMe(c, true)
                commit(machines.filter { h -> list.none { it.host == h.host } } + list); adding = false
                Toast.makeText(c, "Connected to ${list.first().name}", Toast.LENGTH_SHORT).show()
            }
            Spacer(Modifier.height(8.dp))
            if (manual) AddComputer(null) { r ->
                if (r != null) { if (machines.isEmpty()) Store.setCallMe(c, true); commit(machines.filter { it.host != r.host } + r); Toast.makeText(c, "Connected to ${r.name}", Toast.LENGTH_SHORT).show(); adding = false }
                manual = false
            } else Row(horizontalArrangement = Arrangement.spacedBy(20.dp)) { TextAction("Enter the address myself") { manual = true }; TextAction("Cancel") { adding = false } }
        } else TextAction("Add a computer or server") { adding = true }
    }
}

/** Your own computers come first; a server is only preselected if it is the only thing connected. */
private fun defaultMachine(machines: List<Machine>, c: android.content.Context): Machine {
    val last = Store.lastMachine(c)
    return machines.firstOrNull { it.host == last && !it.secure } ?: machines.firstOrNull { !it.secure } ?: machines.first()
}

/** Browse the folders of the server, pick one to work in, or make a new one. */
@Composable
private fun FolderBrowser(m: Machine, start: String, onPick: (String) -> Unit, onClose: () -> Unit) {
    val scope = rememberCoroutineScope()
    var cur by remember { mutableStateOf<Relay.Listing?>(null) }
    var err by remember { mutableStateOf<String?>(null) }
    var hidden by remember { mutableStateOf(false) }
    var name by remember { mutableStateOf("") }
    fun load(p: String) { scope.launch { runCatching { withContext(Dispatchers.IO) { Relay.browse(m, p, hidden) } }.onSuccess { cur = it; err = null }.onFailure { err = it.message ?: "Can't open that folder" } } }
    LaunchedEffect(Unit) { load(start) }
    BackHandler { onClose() }
    Screen {
        Header("Pick a folder") { TextAction("Close", onClose) }
        Txt(m.name + (cur?.let { " · " + it.path } ?: ""), T.small.copy(fontFamily = FontFamily.Monospace), muted = true)
        err?.let { Txt(it, T.small, color = MaterialTheme.colorScheme.error) }
        val l = cur
        if (l != null) {
            Button("Work in this folder", Modifier.fillMaxWidth()) { onPick(l.path) }
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.weight(1f)) { Field(name, { name = it }, "New folder name") }
                TextAction("Make") { if (name.isNotBlank()) scope.launch { runCatching { withContext(Dispatchers.IO) { Relay.makeDir(m, l.path, name.trim()) } }.onSuccess { name = ""; load(it) }.onFailure { err = it.message } } }
            }
            ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Chip("Show hidden folders", hidden) { hidden = !hidden; load(l.path) } }
            if (l.parent != null) Card(Modifier.clickable { load(l.parent) }) { Txt("↑ ..", T.body, muted = true) }
            l.dirs.forEach { d -> Card(Modifier.clickable { load(d.path) }) { Row(verticalAlignment = Alignment.CenterVertically) { Txt(d.name, T.body, modifier = Modifier.weight(1f)); if (d.git) Txt("git", T.small, muted = true) } } }
            if (l.dirs.isEmpty()) Txt("No folders in here.", T.small, muted = true)
        } else if (err == null) Txt("Loading…", T.small, muted = true)
    }
}

@Composable
private fun NewSession(machines: List<Machine>, back: () -> Unit, start: (Machine, String, String, Boolean) -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var sel by remember { mutableStateOf(defaultMachine(machines, c)) }
    var root by remember { mutableStateOf<String?>(null) }
    var projects by remember { mutableStateOf<List<Project>?>(null) }
    var err by remember { mutableStateOf<String?>(null) }
    var name by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var talk by remember { mutableStateOf(false) }
    val m = sel
    LaunchedEffect(m) {
        projects = null; root = null; err = null
        runCatching { withContext(Dispatchers.IO) { Relay.projects(m) } }.onSuccess { root = it.first; projects = it.second }.onFailure { err = it.message ?: "Can't reach ${m.name}" }
    }
    var opts by remember(m) { mutableStateOf<Relay.Options?>(null) }
    var asRoot by remember(m) { mutableStateOf(false) }
    var browse by remember { mutableStateOf(false) }
    LaunchedEffect(m) { opts = if (m.secure) runCatching { withContext(Dispatchers.IO) { Relay.options(m) } }.getOrNull() else null }
    fun go(path: String) { if (!m.secure) Store.setLastMachine(c, m.host); start(m, path, "code", asRoot && opts?.root == true) }
    if (browse) { FolderBrowser(m, root ?: "", onPick = { browse = false; go(it) }, onClose = { browse = false }); return }
    Screen {
        Header("New session") { TextAction("Back", back) }
        ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Chip("Coding", !talk) { talk = false }
            Chip("Talk", talk) { talk = true }
        }
        if (machines.size > 1) {
            Label("Where should it run?")
            ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                machines.sortedBy { it.secure }.forEach { mm -> Chip(mm.name, mm == m) { sel = mm } }
            }
            if (m.secure) {
                var pcs by remember(m) { mutableStateOf<List<Computer>>(emptyList()) }
                LaunchedEffect(m) { Ui.newRun = "vps"; runCatching { withContext(Dispatchers.IO) { Relay.computers(m) } }.onSuccess { pcs = it.filter { x -> x.kind == "computer" } } }
                if (pcs.isNotEmpty()) {
                    Label("Which computer should work on it?")
                    ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        (listOf("Server" to "vps") + pcs.map { (if (it.online) "● " else "○ ") + it.name to it.id }).forEach { (l, v) -> Chip(l, Ui.newRun == v) { Ui.newRun = v } }
                    }
                } else Txt("Runs on the server, so it keeps going when your computer is off.", T.small, muted = true)
            }
        }
        err?.let { Card { Txt(it, T.small, color = MaterialTheme.colorScheme.primary) } }
        if (talk) {
            Card {
                Label("Just talking")
                Spacer(Modifier.height(6.dp))
                Txt("A normal conversation, no project. It uses the cheaper Haiku model and can't change any files, so it costs far fewer tokens.", T.small, muted = true)
                Spacer(Modifier.height(12.dp))
                Button("Start talking", Modifier.fillMaxWidth()) { start(m, "", "talk", false) }
            }
            return@Screen
        }
        val o = opts
        if (o != null && ((o.root && o.rootOk) || (o.outside && o.outsideAvailable))) Card {
            Label("On the server")
            Spacer(Modifier.height(8.dp))
            if (o.outside && o.outsideAvailable) { Button("Browse the server…", Modifier.fillMaxWidth()) { browse = true }; Spacer(Modifier.height(4.dp)) }
            if (o.root && o.rootOk) {
                ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Chip("Run as root", asRoot) { asRoot = !asRoot } }
                if (asRoot) { Spacer(Modifier.height(6.dp)); Txt("Root chat: Claude gets full administrator rights on this server and knows it. Be sure of what you ask.", T.small, color = MaterialTheme.colorScheme.error) }
            }
        }
        if (root != null) Card {
            Label("New project")
            Spacer(Modifier.height(10.dp)); Field(name, { name = it }, "Name, e.g. budget-app")
            Spacer(Modifier.height(10.dp))
            Button(if (busy) "Creating…" else "Create and start", Modifier.fillMaxWidth()) {
                if (name.isBlank() || busy) return@Button
                busy = true; err = null
                scope.launch {
                    runCatching { withContext(Dispatchers.IO) { Relay.createProject(m, name.trim()) } }.onSuccess { go(it) }.onFailure { err = it.message }
                    busy = false
                }
            }
        }
        val ps = projects
        if (ps == null && err == null) Txt("Loading…", T.small, muted = true)
        if (ps != null && ps.isNotEmpty()) { Label(if (root != null) "Or continue a project" else "Recent projects"); ps.forEach { p ->
            Card(Modifier.clickable { go(p.path) }) { Txt(p.name, T.h2); Txt(short(p.path), T.small, muted = true) }
        } }
        if (ps != null && ps.isEmpty() && root == null) Txt("No projects on ${m.name} yet. Start a chat in a folder from your computer first.", T.small, muted = true)
    }
}

// ───────────────────────── chat ─────────────────────────

private class Part(val tool: Boolean, val text: String = "", val name: String = "", val detail: String = "", val id: String = "", val result: String? = null, val err: Boolean = false)

private fun sess0Shared(s: Sess) = s.shared >= 0

@Composable
private fun Chat(m: Machine, s0: Sess, startVoice: Boolean = false, onClear: () -> Unit = {}, back: () -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var sess by remember { mutableStateOf(s0) }
    val msgs = remember { mutableStateListOf<Msg>() }
    val settled = remember { mutableIntStateOf(-1) }   // how many messages were there when the chat opened: later ones glide in
    val unsent = remember { mutableStateListOf<Pair<String, Long>>() }   // sent, but not in the computer's copy of the chat yet: they stay visible until they show up
    val parts = remember { mutableStateListOf<Part>() }
    var sending by remember { mutableStateOf(false) }
    var serverBusy by remember { mutableStateOf(false) }
    var input by remember { mutableStateOf("") }
    var err by remember { mutableStateOf<String?>(null) }
    var mode by remember { mutableStateOf(if (m.secure) "auto" else "acceptEdits") }
    var call by remember { mutableStateOf("off") }
    var run by remember { mutableStateOf("auto") }
    var options by remember { mutableStateOf(false) }
    val appCtx = LocalContext.current
    var model by remember { mutableStateOf(Store.chatModel(appCtx)) }
    var effort by remember { mutableStateOf(Store.chatEffort(appCtx)) }
    var asks by remember { mutableStateOf<List<CallEvent>>(emptyList()) }
    val guest = m.guest   // a shared chat of a friend: you read it, what you send goes to them to approve
    var gInfo by remember { mutableStateOf<GuestInfo?>(null) }
    var shareOpen by remember { mutableStateOf(false) }
    var isShared by remember { mutableStateOf(sess0Shared(s0)) }
    var choiceRev by remember { mutableIntStateOf(0) }
    var optionsOpen by remember { mutableStateOf(false) }
    var ownBusy by remember { mutableStateOf(false) }
    val sharedChat = guest || isShared
    val shareKey = gInfo?.shareId.orEmpty()
    // whose usage a prompt of mine uses in this chat: mine, the host's, or another person's
    val choice = if (sharedChat && gInfo != null && shareKey.isNotEmpty()) { choiceRev; Shares.choice(c, shareKey, guest, gInfo!!.people) } else if (guest) "host" else "mine"
    val files = remember { mutableStateListOf<Pair<String, String>>() }   // (name, path on the computer)
    var uploading by remember { mutableStateOf(false) }
    val ctx = LocalContext.current
    val pick = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) scope.launch {
            uploading = true; err = null
            runCatching {
                withContext(Dispatchers.IO) {
                    val name = ctx.contentResolver.query(uri, null, null, null, null)?.use { cur -> val i = cur.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME); if (cur.moveToFirst() && i >= 0) cur.getString(i) else null } ?: "photo.jpg"
                    val bytes = ctx.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: throw ApiException(0, "Couldn't read that file")
                    if (bytes.size > 25_000_000) throw ApiException(0, "That file is over 25 MB")
                    name to Relay.upload(m, sess.id, sess.cwd, name, bytes)
                }
            }.onSuccess { files.add(it) }.onFailure { err = it.message ?: "Upload failed" }
            uploading = false
        }
    }
    var voice by remember { mutableStateOf(startVoice || (CallSession.active && CallSession.sessId == s0.id)) }
    val list = rememberLazyListState()
    val cs = MaterialTheme.colorScheme
    DisposableEffect(Unit) { Ui.chatOpen = true; onDispose { Ui.chatOpen = false } }
    if (shareOpen) ShareDialog(m, sess.id, sess.title) { shareOpen = false; isShared = true }
    if (optionsOpen) UsageDialog(gInfo, guest, shareKey, choice, onChanged = { choiceRev++ }) { optionsOpen = false }
    BackHandler { back() }

    suspend fun reload() {
        if (sess.id.isEmpty()) return
        runCatching { withContext(Dispatchers.IO) { Relay.messages(m, sess.id) } }
            .onSuccess { raw ->
                val norm = { t: String -> t.replace(Regex("\\s+"), " ").trim() }
                val seen = raw.filter { x -> x.role == "user" }.takeLast(40).map { x -> norm(x.text) }
                unsent.removeAll { u -> val t = norm(u.first); System.currentTimeMillis() - u.second > 30 * 60_000 || seen.any { x -> x == t || (t.length > 24 && (x.contains(t) || (t.contains(x) && x.length > 24))) } }
                val it = raw + unsent.map { u -> Msg("user", u.first) }
                if (it.size != msgs.size || it.lastOrNull()?.text != msgs.lastOrNull()?.text || it.lastOrNull()?.result != msgs.lastOrNull()?.result) { msgs.clear(); msgs.addAll(it) }; if (settled.intValue < 0) settled.intValue = msgs.size; err = null }
            .onFailure { err = it.message ?: "Can't reach ${m.name}" }
    }
    LaunchedEffect(sess.id) {
        // the messages come first and on their own: the small extras load beside them instead of in front of them
        if (sess.id.isNotEmpty()) {
            launch { runCatching { withContext(Dispatchers.IO) { Relay.prefs(m, sess.id) } }.onSuccess { call = it } }
            launch { runCatching { withContext(Dispatchers.IO) { Relay.runPref(m, sess.id) } }.onSuccess { run = it } }
        }
        var round = 0
        while (sess.id.isNotEmpty()) {
            launch { runCatching { withContext(Dispatchers.IO) { Relay.pending(m) } }.onSuccess { l -> asks = l.filter { it.session == sess.id && (it.kind == "question" || it.kind == "permission" || it.kind == "request") } } }
            launch {
                val w = withContext(Dispatchers.IO) { Relay.working(m, sess.id) }
                if (w != null) serverBusy = w
                else if (round++ % 3 == 0) runCatching { withContext(Dispatchers.IO) { Relay.sessions(m) } }.onSuccess { l -> serverBusy = l.firstOrNull { it.id == sess.id }?.let { it.running || it.busy } == true }   // an older server: ask the whole list
            }
            if (guest) launch { runCatching { withContext(Dispatchers.IO) { Shares.guestInfo(m, sess.id) } }.onSuccess { gInfo = it } }
            else if (isShared) launch { runCatching { withContext(Dispatchers.IO) { Shares.hostInfo(m, sess.id) } }.onSuccess { gInfo = it; if (it == null) isShared = false } }
            if (!sending) reload()
            delay(3_000)
        }
    }
    val total = msgs.size + parts.size
    LaunchedEffect(total, parts.lastOrNull()?.text?.length) { if (total > 0) list.scrollToItem(total) }

    val queued = remember { mutableStateListOf<String>() }   // messages written while Claude is busy: they go out one by one when it is free
    fun doSend(text: String) {
        sending = true; parts.clear(); err = null
        if (!guest) { msgs.add(Msg("user", text)); unsent.add(text to System.currentTimeMillis()) }   // a friend's prompt only shows once the host runs it
        val isNew = sess.id.isEmpty()
        scope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    Relay.send(m, sess.id, text, mode, cwd = if (isNew) sess.cwd else null, kind = if (isNew && sess.kind == "talk") "talk" else null, model = model.ifBlank { null }, effort = effort.ifBlank { null }, run = if (isNew && m.secure && Ui.newRun != "vps") Ui.newRun else null, root = isNew && sess.root) { t, tx, e ->
                        when (t) {
                            "session" -> if (isNew && tx.isNotEmpty()) sess = sess.copy(id = tx, title = text.take(60))
                            "prefs" -> call = tx
                            "queued" -> err = if (tx.startsWith("approval:")) "Sent to ${tx.substringAfter(":")}. They have to approve it first, and may change it." else if (tx.startsWith("auto:")) "It runs at once: ${tx.substringAfter(":")} gave you full permission." else if (tx == "now") "Sent to your laptop. It runs there now. Progress appears here in a few seconds." else "Queued. It runs on your laptop as soon as it is online."
                            "delta" -> { val l = parts.lastOrNull(); if (l != null && !l.tool) parts[parts.size - 1] = Part(false, l.text + tx) else parts.add(Part(false, tx)) }
                            "tool" -> { val f = tx.split('\u0000'); parts.add(Part(true, name = f.getOrElse(0) { "" }, detail = f.getOrElse(1) { "" }, id = f.getOrElse(2) { "" })) }
                            "result" -> { val f = tx.split('\u0000', limit = 2); val i = parts.indexOfFirst { it.tool && it.id == f[0] }
                                if (i >= 0) parts[i] = Part(true, name = parts[i].name, detail = parts[i].detail, id = parts[i].id, result = f.getOrElse(1) { "" }, err = e) }
                            "done" -> if (e) err = "Claude reported an error."
                        }
                    }
                }
            }.onFailure {
                if (it is ApiException && it.code == 409 && (it.message ?: "").contains("still working")) {   // busy from somewhere else: wait for it, then send
                    queued.add(0, text); unsent.removeAll { u -> u.first == text }
                    val ix = msgs.indexOfLast { x -> x.role == "user" && x.text == text }; if (ix >= 0) msgs.removeAt(ix)
                    serverBusy = true
                } else err = it.message ?: "Connection lost"
            }
            parts.clear(); sending = false; reload()
        }
    }
    /** Your own usage in a shared chat: the prompt runs on your own server, and the answer is posted into the chat. [by]/[req]: someone asked for it and you said yes. */
    fun runOwnShared(t: String, by: String? = null, req: String? = null, mode: String = "acceptEdits") {
        val gi = gInfo ?: return
        val own = Store.machines(c).firstOrNull { it.secure }
        if (own == null) { err = "Your own usage runs on your own server. Connect one first (Settings)."; return }
        if (ownBusy) { err = "Your Claude is still working on the last one."; return }
        ownBusy = true
        val (sid0, seen) = Shares.ownSession(c, gi.shareId); val dir = Shares.cwd(c, gi.shareId)
        val others = msgs.filter { it.ts > seen && (it.role == "user" || it.role == "assistant") && !(it.own && it.name.startsWith(gi.myName + "'s Claude")) }.takeLast(40)
        val prompt = "You are in a shared chat called \"${sess.title}\" with ${gi.people.joinToString(", ") { it.name }}. " + (if (by != null) "The next message comes from $by, not from the owner of this account. Answer it directly." else "Answer ${gi.myName} directly.") +
            (if (others.isNotEmpty()) "\n\nWhat was said in the chat since your last turn:\n" + others.joinToString("\n") { "${it.name.ifBlank { if (it.role == "user") "Someone" else "Claude" }}: ${it.text}" } else "") + "\n\n${by ?: gi.myName}: $t"
        scope.launch(Dispatchers.IO) {
            val out = StringBuilder(); var failed = ""; var newSid = sid0
            runCatching {
                Shares.postJson(m, "/sessions/${sess.id}/post", JSONObject().put("role", "user").put("text", t).apply { if (req != null) put("req", req) })
                Shares.postJson(m, "/sessions/${sess.id}/working", JSONObject().put("on", true))
                Relay.send(own, sid0, prompt, mode, cwd = if (sid0.isEmpty() && dir.isNotEmpty()) dir else null, kind = if (sid0.isEmpty() && dir.isEmpty()) "talk" else null, run = "vps") { ty, tx, _ ->
                    when (ty) { "session" -> if (tx.isNotEmpty()) newSid = tx; "delta" -> out.append(tx); "fail" -> failed = tx.ifBlank { "Your Claude failed" } }
                }
            }.onFailure { failed = it.message ?: "Your Claude failed" }
            Shares.setOwnSession(c, gi.shareId, newSid, System.currentTimeMillis() - 1)
            runCatching { if (out.isNotBlank() || failed.isNotEmpty()) Shares.postJson(m, "/sessions/${sess.id}/post", JSONObject().put("role", "assistant").put("text", out.toString().trim().ifEmpty { "($failed)" }).apply { if (req != null) put("req", req) }) }
            runCatching { Shares.postJson(m, "/sessions/${sess.id}/working", JSONObject().put("on", false)) }
            if (req != null) runCatching { Shares.postJson(m, "/sessions/${sess.id}/requests/$req", JSONObject().put("decision", if (failed.isEmpty()) "done" else "failed").put("note", failed)) }
            withContext(Dispatchers.Main) { ownBusy = false; reload() }
        }
    }
    fun send() {
        val text = (input.trim() + files.joinToString("") { "\n@" + it.second }).trim()
        if (text.isEmpty() || uploading) return
        if (text.equals("/clear", ignoreCase = true)) { input = ""; files.clear(); onClear(); return }   // like /clear in Claude Code: a fresh start in the same project
        input = ""; files.clear()
        if (sharedChat && gInfo != null) {   // a shared chat: the prompt goes to whoever's usage is chosen
            val others = gInfo!!.people.firstOrNull { it.id == choice }
            if (guest && choice == "host") { doSend(text); return }
            if (guest && choice == "mine") { runOwnShared(text); return }
            if (choice != "mine" && others != null) { scope.launch { runCatching { withContext(Dispatchers.IO) { if (guest) Shares.askFromGuest(m, sess.id, choice, text) else Shares.askHostShare(m, gInfo!!.shareId, choice, text) } }.onSuccess { err = "Sent to ${others.name}. They have to approve it first, then their own Claude answers here." }.onFailure { err = it.message } }; return }
        }
        if (sending || serverBusy) queued.add(text) else doSend(text)
    }
    // a friend you gave "always run" for: their request to use your Claude runs without a question
    val handledInc = remember { mutableSetOf<String>() }
    LaunchedEffect(gInfo?.incoming?.map { it.id + it.status }) {
        val gi = gInfo ?: return@LaunchedEffect
        if (guest) gi.incoming.filter { it.status == "pending" && it.id !in handledInc }.forEach { q -> Shares.autoMode(c, gi.shareId, q.from)?.let { md -> handledInc.add(q.id); runCatching { withContext(Dispatchers.IO) { Shares.postJson(m, "/sessions/${sess.id}/requests/${q.id}", JSONObject().put("decision", "approve").put("text", q.text).put("mode", md)) } }.onSuccess { runOwnShared(q.text, q.fromName, q.id, md) } } }
    }
    LaunchedEffect(serverBusy, sending, queued.size) { if (!serverBusy && !sending && queued.isNotEmpty()) doSend(queued.removeAt(0)) }
    fun setCall(v: String) { val nv = if (call == v) "off" else v; call = nv; if (sess.id.isNotEmpty()) scope.launch(Dispatchers.IO) { runCatching { Relay.setPrefs(m, sess.id, nv) } } }

    if (voice && sess.id.isNotEmpty()) { CallScreen(m, sess, mode) { voice = false; scope.launch { reload() } }; return }
    val working = sending || serverBusy
    Column(Modifier.fillMaxSize().imePadding().padding(top = 12.dp)) {
        Row(Modifier.padding(horizontal = 16.dp).padding(bottom = 12.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(36.dp).clip(RoundedCornerShape(12.dp)).border(1.dp, cs.outline, RoundedCornerShape(12.dp)).clickable(onClick = back), contentAlignment = Alignment.Center) { Txt("‹", T.body.copy(fontSize = 20.sp), muted = true) }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(sess.title, style = T.h2.copy(fontSize = 17.sp), color = cs.onSurface, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    if (working) { Box(Modifier.size(6.dp).clip(RoundedCornerShape(50)).background(cs.primary)); Spacer(Modifier.width(6.dp)) }
                Txt(listOf(if (guest) "Shared by ${m.label}" else if (sess.kind == "talk") "Talk" else folder(sess.cwd), if (guest) null else m.name, if (sess.root) "root" else null, if (call != "off") "will call you" else null, if (working) "working" else null).filterNotNull().joinToString(" · "), T.small.copy(fontSize = 12.sp), muted = true)
                }
            }
            if (sess.id.isNotEmpty() && sharedChat) { TextAction("Options") { optionsOpen = true }; Spacer(Modifier.width(16.dp)) }
            if (sess.id.isNotEmpty() && !guest) { TextAction("Share") { shareOpen = true }; Spacer(Modifier.width(16.dp)) }
            if (sess.id.isNotEmpty() && !guest) { TextAction("Talk") { voice = true }; Spacer(Modifier.width(16.dp)) }
            if (!guest) { TextAction(if (options) "Done" else "Options") { options = !options }; Spacer(Modifier.width(16.dp)) }
            TextAction("Close", back)
        }
        androidx.compose.animation.AnimatedVisibility(options, enter = androidx.compose.animation.expandVertically() + androidx.compose.animation.fadeIn(), exit = androidx.compose.animation.shrinkVertically() + androidx.compose.animation.fadeOut()) { Column(Modifier.padding(horizontal = 20.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Label("How much can Claude do on its own?")
            ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("Auto" to "auto", "Edits" to "acceptEdits", "Plan only" to "plan", "Ask me" to "manual", "Full auto" to "bypassPermissions").forEach { (l, v) -> Chip(l, mode == v) { mode = v; if (sess.id.isNotEmpty()) scope.launch(Dispatchers.IO) { runCatching { Relay.setAuto(m, sess.id, v == "bypassPermissions") }.onFailure { err = it.message ?: "Couldn't change Full auto" } } } }
            }
            if (mode == "bypassPermissions") Txt("Full auto lets Claude run any command on ${m.name} without asking.", T.small, color = cs.primary)
            if (m.secure) {
                Label("Run on")
                var pcs by remember { mutableStateOf<List<Computer>>(emptyList()) }
                LaunchedEffect(options) { runCatching { withContext(Dispatchers.IO) { Relay.computers(m) } }.onSuccess { pcs = it.filter { x -> x.kind == "computer" } } }
                ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    (listOf("Auto" to "auto", "Server" to "vps") + pcs.map { (if (it.online) "● " else "○ ") + it.name to it.id }).forEach { (l, v) -> Chip(l, run == v) { run = v; scope.launch(Dispatchers.IO) { runCatching { Relay.setRun(m, sess.id, v) } } } }
                }
                Txt("Pick the computer that works on this chat. Auto: whichever of your computers is online, otherwise the server. A computer that is off starts when you turn it on. You can also just ask Claude to do something on another PC.", T.small, muted = true)
            }
            if (sess.kind != "talk") {
                Label("Model")
                ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf("Default" to "", "Opus" to "opus", "Sonnet" to "sonnet", "Haiku" to "haiku").forEach { (l, v) -> Chip(l, model == v) { model = v; Store.setChatModel(appCtx, v) } }
                }
                Label("Effort")
                ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf("Default" to "", "Low" to "low", "Medium" to "medium", "High" to "high", "Extra high" to "xhigh", "Max" to "max").forEach { (l, v) -> Chip(l, effort == v) { effort = v; Store.setChatEffort(appCtx, v) } }
                }
                Txt("More effort means deeper thinking and more tokens. Talk sessions always use Haiku.", T.small, muted = true)
            }
            Label("Phone call")
            ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Chip("When it's done", call == "done") { setCall("done") }
                Chip("Also if it needs me", call == "attention") { setCall("attention") }
            }
            Txt("You always get a message when Claude finishes or needs you. A call only happens if you pick one here.", T.small, muted = true)
        } }
        androidx.compose.foundation.text.selection.SelectionContainer(Modifier.weight(1f)) {   // press and hold on any text to select and copy it
        LazyColumn(Modifier.fillMaxSize().padding(horizontal = 20.dp), state = list, verticalArrangement = Arrangement.spacedBy(10.dp), contentPadding = PaddingValues(vertical = 12.dp)) {
            if (msgs.isEmpty() && parts.isEmpty() && sess.id.isEmpty()) item { Txt(if (sess.kind == "talk") "What's on your mind?" else "What should we build in ${folder(sess.cwd)}?", T.title, modifier = Modifier.padding(top = 24.dp)) }
            itemsIndexed(msgs) { idx, msg ->
              Box(if (settled.intValue in 0..idx) Modifier.appear() else Modifier) {
                when (msg.role) {
                    "user" -> if (msg.own) Column(Modifier.fillMaxWidth()) { Txt(msg.name, T.small.copy(fontSize = 12.sp), muted = true); Box(Modifier.widthIn(max = 320.dp).clip(RoundedCornerShape(18.dp)).background(if (cs.background.luminance() < .5f) Color(0xFF2E2A26) else Color(0xFFE9E2D2)).padding(horizontal = 14.dp, vertical = 11.dp)) { Txt(msg.text, T.body) } } else Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
                        Box(Modifier.widthIn(max = 320.dp).clip(RoundedCornerShape(18.dp, 18.dp, 4.dp, 18.dp)).background(if (cs.background.luminance() < .5f) Color(0xFF2E2A26) else Color(0xFFE9E2D2)).padding(horizontal = 14.dp, vertical = 11.dp)) { Txt(msg.text, T.body) }
                    }
                    "tool" -> ToolCard(msg.name, msg.detail.ifEmpty { msg.text }, msg.result, msg.err)
                    "meta" -> Txt(msg.text, T.small, muted = true)
                    else -> if (msg.own) Column { Txt(msg.name + " · own account", T.small.copy(fontSize = 12.sp), muted = true); MarkdownText(msg.text) } else MarkdownText(msg.text)
                }
              }
            }
            itemsIndexed(parts) { _, p -> if (p.tool) ToolCard(p.name, p.detail, p.result, p.err) else MarkdownText(p.text) }
            queued.forEachIndexed { qi, q -> item("q$qi") { Txt("Queued · " + q.take(80), T.small, muted = true) } }
            if (working && parts.isEmpty()) item { Txt("Claude is working…", T.small, muted = true) }
            if (!working && sess.id.isNotEmpty() && msgs.lastOrNull { it.role != "meta" }?.role == "assistant") item {
                ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Chip("Open a PR", false) { input = "Open a pull request with these changes."; send() }
                    Chip("Show diff", false) { input = "Show me the diff of what you changed."; send() }
                }
            }
            if (sharedChat) gInfo?.let { gi -> item {
                Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    gi.reqs.filter { System.currentTimeMillis() - it.ts < 15 * 60_000L || it.status == "pending" }.take(4).reversed().forEach { q ->
                        val label = when (q.status) { "pending" -> "Waiting for ${q.toName.ifBlank { gi.host }} to approve"; "approved" -> "Approved, starting"; "running" -> "Running"; "done" -> "Done"; "denied" -> "Not run"; else -> "Failed" }
                        Txt("$label${if (q.edited) " (changed by ${q.toName.ifBlank { gi.host }})" else ""}: \"${q.text.take(70)}\"", T.small, color = if (q.status == "pending") cs.primary else cs.onSurfaceVariant)
                    }
                    if (guest && gi.auto && choice == "host") Txt("${gi.host} gave you full permission: what you send runs at once.", T.small, color = cs.primary)
                }
            } }
            item { Spacer(Modifier.height(4.dp)) }
        }
        }
        err?.let { Txt(it, T.small, color = cs.primary, modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp)) }
        asks.filter { it.kind != "permission" && it.kind != "request" }.forEach { a -> AskCard(m, a) { asks = asks - a } }
        if (files.isNotEmpty() || uploading) Row(Modifier.padding(horizontal = 20.dp).horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            files.toList().forEach { f -> Chip("${f.first.takeLast(24)}  ×", true) { files.remove(f) } }
            if (uploading) Txt("Uploading…", T.small, muted = true, modifier = Modifier.padding(vertical = 8.dp))
        }
        val perm = asks.firstOrNull { it.kind == "permission" }
        val reqAsk = asks.firstOrNull { it.kind == "request" && it.forMember.isEmpty() }
        val incoming = if (guest) gInfo?.incoming?.firstOrNull { it.status == "pending" && it.id !in handledInc } else null
        if (perm != null) PermDock(m, perm) { asks = asks - perm }   // the approval sits right on top of the message box, like in a terminal agent
        else if (reqAsk != null) RequestDock(m, reqAsk) { asks = asks - reqAsk }   // a friend wants to use your Claude
        else if (incoming != null) IncomingDock(gInfo!!, incoming, onDeny = { scope.launch { runCatching { withContext(Dispatchers.IO) { Shares.postJson(m, "/sessions/${sess.id}/requests/${incoming.id}", JSONObject().put("decision", "deny")) } } } }) { text, mode, always ->
            handledInc.add(incoming.id); if (always) Shares.setAuto(c, gInfo!!.shareId, incoming.from, mode)
            scope.launch { runCatching { withContext(Dispatchers.IO) { Shares.postJson(m, "/sessions/${sess.id}/requests/${incoming.id}", JSONObject().put("decision", "approve").put("text", text).put("mode", mode)) } }.onSuccess { runOwnShared(text, incoming.fromName, incoming.id, mode) }.onFailure { err = it.message } }
        }   // a person asks to use your Claude here
        val boxShape = if (perm != null || reqAsk != null || incoming != null) RoundedCornerShape(0.dp, 0.dp, 24.dp, 24.dp) else RoundedCornerShape(24.dp)
        Box(Modifier.fillMaxWidth().background(cs.background).padding(start = 14.dp, end = 14.dp, bottom = 12.dp, top = if (perm != null || reqAsk != null || incoming != null) 0.dp else 8.dp)) {
            Row(Modifier.fillMaxWidth().clip(boxShape).background(cs.surfaceVariant).border(1.dp, cs.outline, boxShape).padding(start = 16.dp, top = 4.dp, bottom = 4.dp, end = 8.dp),
                verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                if (!guest) TextAction("Attach") { pick.launch("*/*") }
                Box(Modifier.weight(1f)) { Field(input, { input = it }, if (guest) "Ask ${m.label}'s Claude… they approve it first" else if (sess.id.isEmpty()) "Describe your idea" else "Message Claude", singleLine = false, bare = true) }
                val stop = working && sess.id.isNotEmpty()
                Box(Modifier.size(38.dp).pressable { if (stop) scope.launch(Dispatchers.IO) { runCatching { Relay.stop(m, sess.id) }.onFailure { err = "Couldn't stop: " + (it.message ?: "no connection") } } else send() }.clip(RoundedCornerShape(50)).background(cs.primary), contentAlignment = Alignment.Center) {
                    Txt(if (stop) "■" else if (sending) "…" else "↑", T.body.copy(fontWeight = FontWeight.Bold), color = cs.onPrimary)
                }
            }
        }
    }
}

/** Claude waiting on you: a permission to allow or deny, or a question with options and a free answer. */
/** The approval prompt, joined to the top of the message box: what Claude wants to do, then Yes / Yes and don't ask again / No. */
@Composable
private fun PermDock(m: Machine, a: CallEvent, done: () -> Unit) {
    val scope = rememberCoroutineScope()
    val cs = MaterialTheme.colorScheme
    val dark = cs.background.luminance() < .5f
    fun decide(b: String) { scope.launch(Dispatchers.IO) { runCatching { CallApi.decide(m, a.req, b) } }; done() }
    val shape = RoundedCornerShape(topStart = 22.dp, topEnd = 22.dp)
    val againLabel = if (a.tool == "Bash") (if (a.suggest.isNotEmpty()) "Yes, and don't ask again for \"${a.suggest}\"" else null) else if (a.tool.isNotEmpty()) "Yes, and don't ask again for ${a.tool} in this chat" else null
    Column(Modifier.fillMaxWidth().background(cs.background).padding(start = 14.dp, end = 14.dp, top = 8.dp)) {
        Column(Modifier.fillMaxWidth().clip(shape).background(cs.surfaceVariant).border(1.dp, cs.primary, shape).padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Label(if (a.tool.isNotEmpty()) "Claude wants to use ${a.tool}" else "Claude needs your OK")
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(if (dark) Color(0xFF141312) else Color(0xFF2A2823)).horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 10.dp)) {
                val lines = if (a.detail.isNotBlank()) a.detail.lines().take(16) else listOf(a.text)
                lines.forEachIndexed { i, l ->
                    val col = when { l.startsWith("+ ") -> Color(0xFF8FD3A5); l.startsWith("- ") -> Color(0xFFF09A8C); a.tool != "Bash" && i == 0 && a.detail.isNotBlank() -> Color(0xFF9A968C); else -> Color(0xFFF1EEE6) }
                    Text(l, style = T.small.copy(fontFamily = FontFamily.Monospace, fontSize = 12.sp), color = col, softWrap = false)
                }
            }
            val opts = listOfNotNull("Yes" to "allow", if (againLabel != null) againLabel to "allow_always" else null, "No" to "deny")
            opts.forEachIndexed { i, (label, b) ->
                Row(Modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).border(1.dp, if (b == "deny") cs.outline else cs.primary.copy(alpha = .6f), RoundedCornerShape(12.dp)).clickable { decide(b) }.padding(horizontal = 12.dp, vertical = 11.dp), verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.clip(RoundedCornerShape(6.dp)).background(if (dark) Color(0xFF2E2A26) else Color(0xFFE9E2D2)).padding(horizontal = 8.dp, vertical = 2.dp)) { Txt("${i + 1}", T.small.copy(fontFamily = FontFamily.Monospace, fontWeight = FontWeight.Bold), color = if (b == "deny") cs.onSurfaceVariant else cs.primary) }
                    Spacer(Modifier.width(12.dp))
                    Txt(label, T.body)
                }
            }
        }
    }
}

@Composable
private fun AskCard(m: Machine, a: CallEvent, done: () -> Unit) {
    val scope = rememberCoroutineScope()
    val cs = MaterialTheme.colorScheme
    var text by remember(a.req) { mutableStateOf("") }
    val picked = remember(a.req) { mutableStateListOf<String>() }
    fun send(answer: String) { scope.launch(Dispatchers.IO) { runCatching { Relay.answer(m, a.req, answer) } }; done() }
    fun decide(b: String) { scope.launch(Dispatchers.IO) { runCatching { CallApi.decide(m, a.req, b) } }; done() }
    Column(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 6.dp).clip(RoundedCornerShape(14.dp)).background(cs.surfaceVariant).border(1.dp, cs.primary, RoundedCornerShape(14.dp)).padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (a.kind == "permission") {
            Label("Claude needs your OK")
            Txt(a.text, T.small.copy(fontFamily = FontFamily.Monospace))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) { Button("Allow") { decide("allow") }; TextAction("Deny") { decide("deny") } }
        } else {
            Label("Claude asks")
            Txt(a.question.ifEmpty { a.text }, T.body)
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                a.options.forEach { o -> Chip(o, o in picked) { if (a.multi) { if (o in picked) picked.remove(o) else picked.add(o) } else send(o) } }
            }
            Field(text, { text = it }, "Or type your own answer")
            Button(if (a.multi) "Send choices" else "Send", Modifier.fillMaxWidth()) { val r = (picked + text.trim()).filter { it.isNotBlank() }.joinToString(", "); if (r.isNotBlank()) send(r) }
        }
    }
}
