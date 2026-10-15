package dev.voer.hearth

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/** Shared chats on the phone: work in one chat with someone else, each with their own Claude account, or ask the host to run a prompt on theirs. */
data class RoomEntry(val key: String, val host: Machine?, val url: String, val token: String, val id: String, val title: String, val hostName: String, val member: String = "", val pending: Int = 0, val people: Int = 0)
data class RoomMsg(val n: Long, val kind: String, val from: String, val name: String, val via: String, val text: String)
data class RoomReq(val id: String, val name: String, val text: String, val status: String, val edited: Boolean)
data class RoomPerson(val id: String, val name: String, val auto: Boolean, val mode: String)
data class RoomView(val title: String, val host: String, val role: String, val me: String, val myName: String, val auto: Boolean, val seq: Long, val msgs: List<RoomMsg>, val people: List<RoomPerson>, val reqs: List<RoomReq>, val working: List<String>)

object Rooms {
    private fun prefs(c: Context) = c.getSharedPreferences("rooms", Context.MODE_PRIVATE)
    fun joined(c: Context): List<RoomEntry> = runCatching {
        val a = JSONArray(prefs(c).getString("joined", "[]"))
        List(a.length()) { a.getJSONObject(it).let { o -> RoomEntry("g:" + o.getString("id") + ":" + o.getString("member"), null, o.getString("url"), o.getString("token"), o.getString("id"), o.getString("title"), o.optString("host"), o.getString("member")) } }
    }.getOrDefault(emptyList())
    fun saveJoined(c: Context, l: List<RoomEntry>) {
        val a = JSONArray(); l.forEach { a.put(JSONObject().put("id", it.id).put("member", it.member).put("url", it.url).put("token", it.token).put("title", it.title).put("host", it.hostName)) }
        prefs(c).edit().putString("joined", a.toString()).apply()
    }
    fun myName(c: Context) = prefs(c).getString("name", "") ?: ""
    fun setMyName(c: Context, n: String) = prefs(c).edit().putString("name", n).apply()
    fun cwd(c: Context, key: String) = prefs(c).getString("cwd:$key", "") ?: ""
    fun setCwd(c: Context, key: String, v: String) = prefs(c).edit().putString("cwd:$key", v).apply()
    fun session(c: Context, key: String) = prefs(c).getString("sess:$key", "") ?: ""
    fun setSession(c: Context, key: String, v: String) = prefs(c).edit().putString("sess:$key", v).apply()
    fun seen(c: Context, key: String) = prefs(c).getLong("seen:$key", 0L)
    fun setSeen(c: Context, key: String, v: Long) = prefs(c).edit().putLong("seen:$key", v).apply()

    /** One call to a room on a server: with the host's own key, or with the guest key from an invite. */
    fun call(e: RoomEntry, method: String, path: String, body: JSONObject? = null): JSONObject {
        val c = URL(e.url + "/rooms" + path).openConnection() as HttpURLConnection
        c.connectTimeout = 8_000; c.readTimeout = 15_000; c.requestMethod = method
        c.setRequestProperty("Authorization", "Bearer " + e.token)
        val send = body != null && method != "DELETE"
        if (send) { c.doOutput = true; c.setRequestProperty("Content-Type", "application/json") }
        try {
            if (send) c.outputStream.use { it.write(body.toString().toByteArray()) }
            val text = (if (c.responseCode in 200..299) c.inputStream else c.errorStream)?.bufferedReader()?.readText().orEmpty()
            val o = runCatching { JSONObject(text.ifBlank { "{}" }) }.getOrDefault(JSONObject())
            if (c.responseCode !in 200..299) throw ApiException(c.responseCode, o.optString("error").ifBlank { "Server said ${c.responseCode}" })
            return o
        } finally { c.disconnect() }
    }
    fun urlOf(m: Machine) = (if (m.secure) "https" else "http") + "://" + m.host + (if (m.port != 443 && m.port != 80) ":" + m.port else "")
    fun hostEntry(m: Machine, id: String, title: String) = RoomEntry("h:$id", m, urlOf(m), m.token, id, title, "")
    fun list(m: Machine): List<RoomEntry> {
        val c = URL(urlOf(m) + "/rooms").openConnection() as HttpURLConnection
        c.connectTimeout = 8_000; c.readTimeout = 15_000; c.setRequestProperty("Authorization", "Bearer " + m.token)
        try {
            if (c.responseCode != 200) return emptyList()
            val a = JSONArray(c.inputStream.bufferedReader().readText())
            return List(a.length()) { a.getJSONObject(it).let { o -> RoomEntry("h:" + o.getString("id"), m, urlOf(m), m.token, o.getString("id"), o.getString("title"), "", "", o.optInt("pending"), o.optInt("members")) } }
        } finally { c.disconnect() }
    }
    fun view(e: RoomEntry, since: Long): RoomView {
        val o = call(e, "GET", "/${e.id}?since=$since")
        val ma = o.getJSONArray("messages"); val pa = o.getJSONArray("members"); val ra = o.getJSONArray("requests"); val wa = o.getJSONArray("working")
        return RoomView(o.getString("title"), o.optString("host"), o.getString("role"), o.getString("me"), o.optString("myName"), o.optJSONObject("grant")?.optBoolean("auto") ?: false, o.optLong("seq"),
            List(ma.length()) { ma.getJSONObject(it).let { m -> RoomMsg(m.getLong("n"), m.optString("kind"), m.optString("from"), m.optString("name"), m.optString("via"), m.optString("text")) } },
            List(pa.length()) { pa.getJSONObject(it).let { m -> RoomPerson(m.getString("id"), m.getString("name"), m.optJSONObject("grant")?.optBoolean("auto") ?: false, m.optJSONObject("grant")?.optString("mode") ?: "plan") } },
            List(ra.length()) { ra.getJSONObject(it).let { q -> RoomReq(q.getString("id"), q.optString("name"), q.optString("text"), q.optString("status"), q.optBoolean("edited")) } },
            List(wa.length()) { wa.getJSONObject(it).optString("name") })
    }
    fun join(link: String, name: String): RoomEntry {
        val u = Uri.parse(link.trim()); val url = u.getQueryParameter("url"); val invite = u.getQueryParameter("invite")
        if (url.isNullOrBlank() || invite.isNullOrBlank()) throw ApiException(0, "That is not an invite link.")
        val o = call(RoomEntry("", null, url, "", "", "", ""), "POST", "/join", JSONObject().put("invite", invite).put("name", name))
        return RoomEntry("g:" + o.getString("room") + ":" + o.getString("member"), null, url, o.getString("token"), o.getString("room"), o.optString("title"), o.optString("host"), o.getString("member"))
    }
}

@Composable
fun RoomsScreen(machines: List<Machine>, joinLink: String?, back: () -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var list by remember { mutableStateOf<List<RoomEntry>>(Rooms.joined(c)) }
    var open by remember { mutableStateOf<RoomEntry?>(null) }
    var form by remember { mutableStateOf(if (joinLink != null) "join" else "") }
    var err by remember { mutableStateOf<String?>(null) }
    var title by remember { mutableStateOf("") }
    var folder by remember { mutableStateOf("") }
    var link by remember { mutableStateOf(joinLink ?: "") }
    var name by remember { mutableStateOf(Rooms.myName(c)) }
    val host = machines.firstOrNull { it.secure } ?: machines.firstOrNull()
    fun reload() { scope.launch { val h = if (host != null) runCatching { withContext(Dispatchers.IO) { Rooms.list(host) } }.getOrDefault(emptyList()) else emptyList(); list = h + Rooms.joined(c) } }
    LaunchedEffect(Unit) { while (true) { reload(); delay(5_000) } }
    val o = open
    if (o != null) { RoomScreen(o) { open = null; reload() }; return }
    BackHandler { if (form.isNotEmpty()) form = "" else back() }
    Screen {
        Header("Shared chats", "Work together, each with your own account") { TextAction("Back", back) }
        err?.let { Txt(it, T.small, color = MaterialTheme.colorScheme.error) }
        if (form == "new") Card {
            Label("Share a chat")
            Spacer(Modifier.height(6.dp)); Txt("Your Claude works in this folder on your server. Friends join with an invite. Nobody gets your login.", T.small, muted = true)
            Spacer(Modifier.height(8.dp)); Field(title, { title = it }, "Name, e.g. Landing page with Sam")
            Spacer(Modifier.height(8.dp)); Field(folder, { folder = it.trim() }, "/path/to/project on your server", mono = true)
            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
                Button("Create") {
                    val m = host ?: return@Button
                    scope.launch {
                        val t = title
                        runCatching { withContext(Dispatchers.IO) { Rooms.call(RoomEntry("", m, Rooms.urlOf(m), m.token, "", "", ""), "POST", "", JSONObject().put("title", t).put("cwd", folder)).getString("id") } }
                            .onSuccess { id -> form = ""; err = null; title = ""; open = Rooms.hostEntry(m, id, t.ifBlank { "Shared chat" }) }.onFailure { err = it.message }
                    }
                }
                TextAction("Cancel") { form = "" }
            }
        } else if (form == "join") Card {
            Label("Join a shared chat")
            Spacer(Modifier.height(8.dp)); Field(link, { link = it.trim() }, "Paste the invite link", mono = true)
            Spacer(Modifier.height(8.dp)); Field(name, { name = it }, "Your name")
            Spacer(Modifier.height(10.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
                Button("Join") {
                    val nm = name.trim().ifBlank { "Guest" }; Rooms.setMyName(c, nm)
                    scope.launch {
                        runCatching { withContext(Dispatchers.IO) { Rooms.join(link, nm) } }.onSuccess { e -> Rooms.saveJoined(c, Rooms.joined(c) + e); form = ""; err = null; reload(); open = e }.onFailure { err = it.message }
                    }
                }
                TextAction("Cancel") { form = "" }
            }
        } else {
            Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                if (host != null) Button("Share a chat", Modifier.weight(1f)) { form = "new" }
                Button("Join", Modifier.weight(1f)) { form = "join" }
            }
            if (list.isEmpty()) Txt("No shared chats yet. Start one, or join with an invite from a friend.", T.small, muted = true)
            list.forEach { e ->
                Card(Modifier.clickable { open = e }) {
                    Txt(e.title, T.body.copy(fontWeight = FontWeight.SemiBold))
                    Txt(if (e.host != null) "You host" + (if (e.people > 0) " · ${e.people} ${if (e.people == 1) "person" else "people"}" else "") + (if (e.pending > 0) " · ${e.pending} waiting" else "") else "With ${e.hostName.ifBlank { "a friend" }}", T.small, muted = true)
                }
            }
        }
    }
}

private val ROOM_MODES = listOf("plan" to "Plan only", "acceptEdits" to "Edits", "manual" to "Ask each step", "bypassPermissions" to "Full auto")

@Composable
private fun RoomScreen(e: RoomEntry, close: () -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    val cs = MaterialTheme.colorScheme
    var v by remember { mutableStateOf<RoomView?>(null) }
    var seq by remember { mutableStateOf(0L) }
    var all by remember { mutableStateOf<List<RoomMsg>>(emptyList()) }
    var err by remember { mutableStateOf<String?>(null) }
    var text by remember { mutableStateOf("") }
    var ownBusy by remember { mutableStateOf(false) }
    var invite by remember { mutableStateOf<String?>(null) }
    val host = e.host != null
    fun poll(force: Boolean = false) {
        scope.launch {
            runCatching { withContext(Dispatchers.IO) { Rooms.view(e, if (force) 0 else seq) } }.onSuccess { nv ->
                all = if (force) nv.msgs else all + nv.msgs.filter { m -> all.none { it.n == m.n } }; seq = nv.seq; v = nv; err = null
            }.onFailure { err = it.message ?: "Can't reach the host" }
        }
    }
    LaunchedEffect(e.key) { while (true) { poll(); delay(2_500) } }
    BackHandler { close() }
    fun act(path: String, body: JSONObject, method: String = "POST", after: () -> Unit = {}) { scope.launch { runCatching { withContext(Dispatchers.IO) { Rooms.call(e, method, "/${e.id}$path", body) } }.onSuccess { after(); poll() }.onFailure { err = it.message } } }
    fun take(): String { val t = text.trim(); text = ""; return t }
    /** The guest's own account: the prompt runs on their own computer or server, the answer is posted to the shared chat. */
    fun runOwn(t: String) {
        val cur = v ?: return
        val own = Store.machines(c).let { l -> l.firstOrNull { !it.secure } ?: l.firstOrNull() }
        if (own == null) { err = "Connect your own computer or server first."; return }
        ownBusy = true
        val seen = Rooms.seen(c, e.key); val sess = Rooms.session(c, e.key); val dir = Rooms.cwd(c, e.key)
        val others = all.filter { it.n > seen && it.from != cur.me && it.kind != "status" }.takeLast(40)
        val prompt = "You are in a shared chat called \"${cur.title}\" with ${(listOf(cur.host) + cur.people.map { it.name }).joinToString(", ")}. Answer ${cur.myName} directly." +
            (if (others.isNotEmpty()) "\n\nWhat was said in the chat since your last turn:\n" + others.joinToString("\n") { "${it.name}: ${it.text}" } else "") + "\n\n${cur.myName}: $t"
        scope.launch(Dispatchers.IO) {
            val out = StringBuilder(); var failed = ""
            runCatching {
                Rooms.call(e, "POST", "/${e.id}/messages", JSONObject().put("text", t)); Rooms.call(e, "POST", "/${e.id}/working", JSONObject().put("on", true))
                Relay.send(own, sess, prompt, "acceptEdits", cwd = if (sess.isEmpty() && dir.isNotEmpty()) dir else null, kind = if (sess.isEmpty() && dir.isEmpty()) "talk" else null) { ty, tx, er ->
                    when (ty) { "session" -> if (tx.isNotEmpty()) Rooms.setSession(c, e.key, tx); "delta" -> out.append(tx); "fail" -> failed = tx.ifBlank { "Your Claude failed" } }
                }
            }.onFailure { failed = it.message ?: "Your Claude failed" }
            Rooms.setSeen(c, e.key, cur.seq)
            runCatching { if (out.isNotBlank() || failed.isNotEmpty()) Rooms.call(e, "POST", "/${e.id}/messages", JSONObject().put("text", out.toString().trim().ifEmpty { "($failed)" }).put("kind", "assistant")) }
            runCatching { Rooms.call(e, "POST", "/${e.id}/working", JSONObject().put("on", false)) }
            withContext(Dispatchers.Main) { ownBusy = false; poll() }
        }
    }
    val cur = v
    Screen {
        Header(cur?.title ?: e.title, if (host) "You host this chat" else "Hosted by " + (cur?.host ?: e.hostName)) {
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                if (host) TextAction("Invite") { scope.launch { runCatching { withContext(Dispatchers.IO) { Rooms.call(e, "POST", "/${e.id}/invite", JSONObject()).getString("invite") } }.onSuccess { invite = "hearth://room?url=" + Uri.encode(e.url) + "&invite=" + it }.onFailure { err = it.message } } }
                TextAction("Back", close)
            }
        }
        err?.let { Txt(it, T.small, color = cs.error) }
        invite?.let { link ->
            Card {
                Label("Invite link")
                Spacer(Modifier.height(6.dp)); Txt("Works once, for 24 hours. Your friend pastes it under Shared chats → Join.", T.small, muted = true)
                Spacer(Modifier.height(6.dp)); Txt(link, T.small.copy(fontFamily = FontFamily.Monospace))
                if (host && e.host != null && !e.host.secure) { Spacer(Modifier.height(6.dp)); Txt("This address only works on your own network. For a friend elsewhere, host the shared chat on your server.", T.small, color = cs.error) }
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                    TextAction("Share") { c.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, link), "Invite")) }
                    TextAction("Close") { invite = null }
                }
            }
        }
        if (cur == null) { Txt("Loading…", T.small, muted = true); return@Screen }
        all.forEach { m ->
            if (m.kind == "status") { Txt(m.text, T.small, muted = true); return@forEach }
            val mine = m.from == cur.me
            val shape = RoundedCornerShape(16.dp)
            Column(Modifier.fillMaxWidth(), horizontalAlignment = if (mine && m.kind != "assistant") Alignment.End else Alignment.Start) {
                Txt(m.name + (when (m.via) { "host" -> " · on the host's account"; "own" -> " · own account"; "ask-host" -> " → asks the host's Claude"; else -> "" }), T.small.copy(fontSize = 12.sp), muted = true)
                val dark = cs.background.luminance() < .5f
                Box(Modifier.widthIn(max = 320.dp).clip(shape).background(if (m.kind == "assistant") cs.surfaceVariant else if (dark) androidx.compose.ui.graphics.Color(0xFF2E2A26) else androidx.compose.ui.graphics.Color(0xFFE9E2D2)).padding(horizontal = 14.dp, vertical = 10.dp)) { Txt(m.text, T.body) }
            }
        }
        cur.working.forEach { Txt("$it is working…", T.small, muted = true) }
        if (host) cur.reqs.filter { it.status == "pending" }.forEach { q ->
            var edit by remember(q.id) { mutableStateOf(q.text) }
            var mode by remember(q.id) { mutableStateOf("plan") }
            var always by remember(q.id) { mutableStateOf(false) }
            Card {
                Label("${q.name} wants to ask your Claude")
                Spacer(Modifier.height(4.dp)); Txt("It runs on your account and your server. You can change the prompt first.", T.small, muted = true)
                Spacer(Modifier.height(8.dp)); Field(edit, { edit = it }, "Prompt", singleLine = false)
                Spacer(Modifier.height(8.dp))
                ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) { ROOM_MODES.forEach { (k, l) -> Chip(l, mode == k) { mode = k } } }
                Spacer(Modifier.height(8.dp))
                Row(verticalAlignment = Alignment.CenterVertically) { Txt("Always run ${q.name}'s prompts without asking", T.small, modifier = Modifier.weight(1f)); Switch(always, { always = it }) }
                Spacer(Modifier.height(10.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Button("Run it") { act("/requests/${q.id}", JSONObject().put("decision", "approve").put("text", edit).put("mode", mode).put("always", always)) }
                    TextAction("Deny") { act("/requests/${q.id}", JSONObject().put("decision", "deny")) }
                }
            }
        } else {
            val waiting = cur.reqs.count { it.status == "pending" }
            if (waiting > 0) Txt("$waiting request${if (waiting > 1) "s" else ""} waiting for ${cur.host} to approve…", T.small, muted = true)
        }
        Field(text, { text = it }, if (host) "Ask your Claude, or just talk to the others…" else "Write something…", singleLine = false)
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically, modifier = Modifier.horizontalScroll(rememberScrollState())) {
            if (host) Button("Ask my Claude") { val t = take(); if (t.isNotEmpty()) act("/ask", JSONObject().put("text", t)) }
            else Button(if (ownBusy) "Working…" else "Send with my account") { val t = take(); if (t.isNotEmpty() && !ownBusy) runOwn(t) }
            if (!host) Chip("Ask ${cur.host}'s Claude", false) { val t = take(); if (t.isNotEmpty()) act("/requests", JSONObject().put("text", t)) }
            Chip("Just say it", false) { val t = take(); if (t.isNotEmpty()) act("/messages", JSONObject().put("text", t)) }
        }
        if (!host) {
            Txt("Asking ${cur.host}'s Claude uses their account. " + (if (cur.auto) "They gave you full permission: it runs at once." else "They decide, and can change your prompt first."), T.small, muted = true)
            var dir by remember { mutableStateOf(Rooms.cwd(c, e.key)) }
            Field(dir, { dir = it.trim(); Rooms.setCwd(c, e.key, dir) }, "Folder for your Claude on your computer (empty = just talk)", mono = true)
            TextAction("Leave this shared chat") { act("/leave", JSONObject(), after = { Rooms.saveJoined(c, Rooms.joined(c).filter { it.key != e.key }); close() }) }
        } else {
            if (cur.people.isNotEmpty()) {
                Label("People in this chat")
                cur.people.forEach { p ->
                    Card {
                        Txt(p.name, T.body.copy(fontWeight = FontWeight.SemiBold))
                        Row(verticalAlignment = Alignment.CenterVertically) { Txt("Runs without asking (full permission)", T.small, modifier = Modifier.weight(1f)); Switch(p.auto, { act("/members/${p.id}", JSONObject().put("grant", JSONObject().put("auto", it).put("mode", p.mode))) }) }
                        ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) { ROOM_MODES.forEach { (k, l) -> Chip(l, p.mode == k) { act("/members/${p.id}", JSONObject().put("grant", JSONObject().put("auto", p.auto).put("mode", k))) } } }
                        TextAction("Remove ${p.name}") { act("/members/${p.id}", JSONObject(), method = "DELETE") }
                    }
                }
            }
            TextAction("Delete this shared chat") { act("", JSONObject(), method = "DELETE", after = { close() }) }
        }
    }
}
