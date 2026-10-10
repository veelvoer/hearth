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

@Composable
fun Field(value: String, onChange: (String) -> Unit, hint: String, mono: Boolean = false, number: Boolean = false, singleLine: Boolean = true, modifier: Modifier = Modifier) {
    val cs = MaterialTheme.colorScheme
    Box(modifier.fillMaxWidth().clip(RoundedCornerShape(12.dp)).background(cs.surface)
        .border(1.dp, cs.outline, RoundedCornerShape(12.dp)).padding(14.dp)) {
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
                Box(Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(cs.surfaceVariant).border(1.dp, cs.outline, RoundedCornerShape(10.dp))
                    .horizontalScroll(rememberScrollState()).padding(12.dp)) { Txt(body, T.small.copy(fontFamily = FontFamily.Monospace)) }
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
    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)).background(cs.surfaceVariant).border(1.dp, cs.outline, RoundedCornerShape(10.dp))
        .clickable { open = !open }.padding(horizontal = 12.dp, vertical = 8.dp)) {
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
    val po = Ui.pendingOpen
    LaunchedEffect(po) {
        if (po != null) {
            Store.machines(c).firstOrNull { it.host == po.host }?.let { view = View.Chat(it, Sess(po.session, po.title, po.cwd, 0, true), po.voice) }
            Ui.pendingOpen = null
        }
    }
    val v = view
    BackHandler(v !is View.Home) { view = View.Home }
    when (v) {
        is View.Home -> Home(machines, { machines = it; Store.saveMachines(c, it); CallService.sync(c) }, { view = View.New }) { m, s -> view = View.Chat(m, s) }
        is View.New -> NewSession(machines, { view = View.Home }) { m, path, kind -> view = View.Chat(m, Sess("", if (kind == "talk") "New talk" else "New session", path, 0, false, kind = kind)) }
        is View.Chat -> Chat(v.m, v.s, v.voice) { view = View.Home }
    }
}

@Composable
private fun Home(machines: List<Machine>, save: (List<Machine>) -> Unit, newSession: () -> Unit, open: (Machine, Sess) -> Unit) {
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
    LaunchedEffect(machines) {
        while (true) {
            machines.forEach { mm -> lists[mm.host] = runCatching { withContext(Dispatchers.IO) { Relay.sessions(mm) } }.getOrNull() }
            delay(6_000)
        }
    }
    Screen {
        Header("Sessions")
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
        Button("New session", Modifier.fillMaxWidth().tourTarget("new")) { newSession() }
        val offline = machines.filter { lists.containsKey(it.host) && lists[it.host] == null }
        if (offline.isNotEmpty()) Txt(offline.joinToString { it.name } + " can't be reached right now. It will reconnect on its own.", T.small, muted = true)
        // the server holds every chat, so its list is the list (same as on the desktop app); other computers only fill in what it lacks
        val hub = machines.firstOrNull { it.secure && lists[it.host] != null }
        val all = machines.flatMap { mm -> (lists[mm.host] ?: emptyList()).map { mm to it } }
            .groupBy { (_, s) -> s.id }.values.map { g ->
                g.firstOrNull { (mm, _) -> mm === hub } ?: g.reduce { a, b -> if (a.second.running != b.second.running) (if (a.second.running) a else b) else if (b.second.mtime > a.second.mtime + 2500) b else a }
            }.sortedByDescending { (_, s) -> if (s.running) Long.MAX_VALUE else s.mtime }.take(60)
        if (all.isEmpty() && offline.size < machines.size && lists.size == machines.size) Txt("No sessions yet. Tap New session to start one.", T.small, muted = true)
        all.forEachIndexed { i, (mm, s) -> Box(if (i == 0) Modifier.tourTarget("list") else Modifier) { SessionCard(s, s.elsewhere.ifBlank { null }?.let { "only on $it" }, now) { open(mm, s) } } }
    }
}

@Composable
private fun SessionCard(s: Sess, machine: String?, now: Long, onClick: () -> Unit) {
    Card(Modifier.pressable(onClick)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            if (s.running || s.busy) Box(Modifier.size(8.dp).clip(RoundedCornerShape(50)).background(MaterialTheme.colorScheme.primary))
            if (s.running || s.busy) Spacer(Modifier.width(8.dp))
            Text(s.title, style = T.h2, color = MaterialTheme.colorScheme.onSurface, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        }
        Spacer(Modifier.height(4.dp))
        Txt(listOfNotNull(if (s.kind == "talk") "Talk" else folder(s.cwd), machine, if (s.running) "working" else dur(now - s.mtime) + " ago").joinToString(" · "), T.small, muted = true)
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

@Composable
private fun NewSession(machines: List<Machine>, back: () -> Unit, start: (Machine, String, String) -> Unit) {
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
    fun go(path: String) { if (!m.secure) Store.setLastMachine(c, m.host); start(m, path, "code") }
    Screen {
        Header("New session") { TextAction("Back", back) }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Chip("Coding", !talk) { talk = false }
            Chip("Talk", talk) { talk = true }
        }
        if (machines.size > 1) {
            Label("Where should it run?")
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                machines.sortedBy { it.secure }.forEach { mm -> Chip(mm.name, mm == m) { sel = mm } }
            }
            if (m.secure) {
                var pcs by remember(m) { mutableStateOf<List<Computer>>(emptyList()) }
                LaunchedEffect(m) { Ui.newRun = "vps"; runCatching { withContext(Dispatchers.IO) { Relay.computers(m) } }.onSuccess { pcs = it.filter { x -> x.kind == "computer" } } }
                if (pcs.isNotEmpty()) {
                    Label("Which computer should work on it?")
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
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
                Button("Start talking", Modifier.fillMaxWidth()) { start(m, "", "talk") }
            }
            return@Screen
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

@Composable
private fun Chat(m: Machine, s0: Sess, startVoice: Boolean = false, back: () -> Unit) {
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
        if (sess.id.isNotEmpty()) runCatching { withContext(Dispatchers.IO) { Relay.prefs(m, sess.id) } }.onSuccess { call = it }
        if (sess.id.isNotEmpty()) runCatching { withContext(Dispatchers.IO) { Relay.runPref(m, sess.id) } }.onSuccess { run = it }
        while (sess.id.isNotEmpty()) {
            if (!sending) reload()
            runCatching { withContext(Dispatchers.IO) { Relay.sessions(m) } }.onSuccess { l -> serverBusy = l.firstOrNull { it.id == sess.id }?.running == true }
            runCatching { withContext(Dispatchers.IO) { Relay.pending(m) } }.onSuccess { l -> asks = l.filter { it.session == sess.id && (it.kind == "question" || it.kind == "permission") } }
            delay(3_000)
        }
    }
    val total = msgs.size + parts.size
    LaunchedEffect(total, parts.lastOrNull()?.text?.length) { if (total > 0) list.scrollToItem(total) }

    fun send() {
        val text = (input.trim() + files.joinToString("") { "\n@" + it.second }).trim()
        if (text.isEmpty() || sending || uploading) return
        input = ""; files.clear(); sending = true; parts.clear(); err = null
        msgs.add(Msg("user", text)); unsent.add(text to System.currentTimeMillis())
        val isNew = sess.id.isEmpty()
        scope.launch {
            runCatching {
                withContext(Dispatchers.IO) {
                    Relay.send(m, sess.id, text, mode, cwd = if (isNew) sess.cwd else null, kind = if (isNew && sess.kind == "talk") "talk" else null, model = model.ifBlank { null }, effort = effort.ifBlank { null }, run = if (isNew && m.secure && Ui.newRun != "vps") Ui.newRun else null) { t, tx, e ->
                        when (t) {
                            "session" -> if (isNew && tx.isNotEmpty()) sess = sess.copy(id = tx, title = text.take(60))
                            "prefs" -> call = tx
                            "queued" -> err = if (tx == "now") "Sent to your laptop. It runs there now. Progress appears here in a few seconds." else "Queued. It runs on your laptop as soon as it is online."
                            "delta" -> { val l = parts.lastOrNull(); if (l != null && !l.tool) parts[parts.size - 1] = Part(false, l.text + tx) else parts.add(Part(false, tx)) }
                            "tool" -> { val f = tx.split('\u0000'); parts.add(Part(true, name = f.getOrElse(0) { "" }, detail = f.getOrElse(1) { "" }, id = f.getOrElse(2) { "" })) }
                            "result" -> { val f = tx.split('\u0000', limit = 2); val i = parts.indexOfFirst { it.tool && it.id == f[0] }
                                if (i >= 0) parts[i] = Part(true, name = parts[i].name, detail = parts[i].detail, id = parts[i].id, result = f.getOrElse(1) { "" }, err = e) }
                            "done" -> if (e) err = "Claude reported an error."
                        }
                    }
                }
            }.onFailure { err = it.message ?: "Connection lost" }
            parts.clear(); sending = false; reload()
        }
    }
    fun setCall(v: String) { val nv = if (call == v) "off" else v; call = nv; if (sess.id.isNotEmpty()) scope.launch(Dispatchers.IO) { runCatching { Relay.setPrefs(m, sess.id, nv) } } }

    if (voice && sess.id.isNotEmpty()) { CallScreen(m, sess, mode) { voice = false; scope.launch { reload() } }; return }
    val working = sending || serverBusy
    Column(Modifier.fillMaxSize().imePadding().padding(top = 12.dp)) {
        Row(Modifier.padding(horizontal = 20.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(sess.title, style = T.h2, color = cs.onSurface, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Txt(listOf(if (sess.kind == "talk") "Talk" else folder(sess.cwd), m.name, if (call != "off") "will call you" else null).filterNotNull().joinToString(" · "), T.small, muted = true)
            }
            if (sess.id.isNotEmpty()) { TextAction("Talk") { voice = true }; Spacer(Modifier.width(16.dp)) }
            TextAction(if (options) "Done" else "Options") { options = !options }
            Spacer(Modifier.width(16.dp))
            TextAction("Close", back)
        }
        androidx.compose.animation.AnimatedVisibility(options, enter = androidx.compose.animation.expandVertically() + androidx.compose.animation.fadeIn(), exit = androidx.compose.animation.shrinkVertically() + androidx.compose.animation.fadeOut()) { Column(Modifier.padding(horizontal = 20.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Label("How much can Claude do on its own?")
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf("Auto" to "auto", "Edits" to "acceptEdits", "Plan only" to "plan", "Ask me" to "manual", "Full auto" to "bypassPermissions").forEach { (l, v) -> Chip(l, mode == v) { mode = v } }
            }
            if (mode == "bypassPermissions") Txt("Full auto lets Claude run any command on ${m.name} without asking.", T.small, color = cs.primary)
            if (m.secure) {
                Label("Run on")
                var pcs by remember { mutableStateOf<List<Computer>>(emptyList()) }
                LaunchedEffect(options) { runCatching { withContext(Dispatchers.IO) { Relay.computers(m) } }.onSuccess { pcs = it.filter { x -> x.kind == "computer" } } }
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    (listOf("Auto" to "auto", "Server" to "vps") + pcs.map { (if (it.online) "● " else "○ ") + it.name to it.id }).forEach { (l, v) -> Chip(l, run == v) { run = v; scope.launch(Dispatchers.IO) { runCatching { Relay.setRun(m, sess.id, v) } } } }
                }
                Txt("Pick the computer that works on this chat. Auto: whichever of your computers is online, otherwise the server. A computer that is off starts when you turn it on. You can also just ask Claude to do something on another PC.", T.small, muted = true)
            }
            if (sess.kind != "talk") {
                Label("Model")
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf("Default" to "", "Opus" to "opus", "Sonnet" to "sonnet", "Haiku" to "haiku").forEach { (l, v) -> Chip(l, model == v) { model = v; Store.setChatModel(appCtx, v) } }
                }
                Label("Effort")
                Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf("Default" to "", "Low" to "low", "Medium" to "medium", "High" to "high", "Extra high" to "xhigh", "Max" to "max").forEach { (l, v) -> Chip(l, effort == v) { effort = v; Store.setChatEffort(appCtx, v) } }
                }
                Txt("More effort means deeper thinking and more tokens. Talk sessions always use Haiku.", T.small, muted = true)
            }
            Label("Phone call")
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Chip("When it's done", call == "done") { setCall("done") }
                Chip("Also if it needs me", call == "attention") { setCall("attention") }
            }
            Txt("You always get a message when Claude finishes or needs you. A call only happens if you pick one here.", T.small, muted = true)
        } }
        LazyColumn(Modifier.weight(1f).padding(horizontal = 20.dp), state = list, verticalArrangement = Arrangement.spacedBy(10.dp), contentPadding = PaddingValues(vertical = 12.dp)) {
            if (msgs.isEmpty() && parts.isEmpty() && sess.id.isEmpty()) item { Txt(if (sess.kind == "talk") "What's on your mind?" else "What should we build in ${folder(sess.cwd)}?", T.title, modifier = Modifier.padding(top = 24.dp)) }
            itemsIndexed(msgs) { idx, msg ->
              Box(if (settled.intValue in 0..idx) Modifier.appear() else Modifier) {
                when (msg.role) {
                    "user" -> Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.CenterEnd) {
                        Box(Modifier.widthIn(max = 320.dp).clip(RoundedCornerShape(14.dp)).background(cs.surfaceVariant).border(1.dp, cs.outline, RoundedCornerShape(14.dp)).padding(12.dp)) { Txt(msg.text, T.body) }
                    }
                    "tool" -> ToolCard(msg.name, msg.detail.ifEmpty { msg.text }, msg.result, msg.err)
                    "meta" -> Txt(msg.text, T.small, muted = true)
                    else -> MarkdownText(msg.text)
                }
              }
            }
            itemsIndexed(parts) { _, p -> if (p.tool) ToolCard(p.name, p.detail, p.result, p.err) else MarkdownText(p.text) }
            if (working && parts.isEmpty()) item { Txt("Claude is working…", T.small, muted = true) }
            item { Spacer(Modifier.height(4.dp)) }
        }
        err?.let { Txt(it, T.small, color = cs.primary, modifier = Modifier.padding(horizontal = 20.dp, vertical = 4.dp)) }
        asks.forEach { a -> AskCard(m, a) { asks = asks - a } }
        if (files.isNotEmpty() || uploading) Row(Modifier.padding(horizontal = 20.dp).horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            files.toList().forEach { f -> Chip("${f.first.takeLast(24)}  ×", true) { files.remove(f) } }
            if (uploading) Txt("Uploading…", T.small, muted = true, modifier = Modifier.padding(vertical = 8.dp))
        }
        Row(Modifier.padding(start = 20.dp, end = 20.dp, bottom = 12.dp, top = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            TextAction("Attach") { pick.launch("*/*") }
            Box(Modifier.weight(1f)) { Field(input, { input = it }, if (sess.id.isEmpty()) "Describe your idea" else "Message Claude", singleLine = false) }
            if (working && sess.id.isNotEmpty()) Button("Stop") { scope.launch(Dispatchers.IO) { runCatching { Relay.stop(m, sess.id) } } }
            else Button(if (sending) "…" else "Send") { send() }
        }
    }
}

/** Claude waiting on you: a permission to allow or deny, or a question with options and a free answer. */
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
