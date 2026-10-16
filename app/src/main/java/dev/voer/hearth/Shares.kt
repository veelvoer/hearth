package dev.voer.hearth

import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

/** Shared chats. A shared chat is a normal chat of someone's server that other people may open. What a friend sends is a request on the host's account:
 *  the host gets a notification, can change the prompt and run it, deny it, or give that friend full permission. */
data class ShareMember(val id: String, val name: String, val auto: Boolean, val mode: String)
data class ShareInfo(val id: String, val members: List<ShareMember>)
data class GuestReq(val id: String, val text: String, val status: String, val ts: Long, val edited: Boolean, val note: String)
data class GuestInfo(val host: String, val auto: Boolean, val reqs: List<GuestReq>)

val SHARE_MODES = listOf("plan" to "Plan only", "acceptEdits" to "Edits", "manual" to "Ask each step", "bypassPermissions" to "Full auto")

object Shares {
    private fun prefs(c: Context) = c.getSharedPreferences("shares", Context.MODE_PRIVATE)

    // The chats you were invited to are a list of items with a time, kept on your own server too, so all your phones and computers show the same chats.
    // The newest write of an item wins. Same keys as the desktop app.
    private fun items(c: Context): JSONObject = runCatching { JSONObject(prefs(c).getString("items", "{}") ?: "{}") }.getOrDefault(JSONObject())
    private fun saveItems(c: Context, o: JSONObject) = prefs(c).edit().putString("items", o.toString()).apply()
    fun set(c: Context, k: String, v: Any?) { val o = items(c); o.put(k, JSONObject().put("v", v ?: JSONObject.NULL).put("t", System.currentTimeMillis())); saveItems(c, o) }
    /** Sends what this device has and takes what the server (and so the other devices) has newer. True when something new arrived. */
    fun sync(c: Context, m: Machine): Boolean {
        val mine = items(c)
        val r = post(m.base, m.token, "/rooms-sync", JSONObject().put("items", mine)).optJSONObject("items") ?: return false
        var changed = false
        r.keys().forEach { k -> val it = r.getJSONObject(k); val cur = mine.optJSONObject(k); if (cur == null || cur.optLong("t") < it.optLong("t")) { mine.put(k, it); changed = true } }
        if (changed) saveItems(c, mine)
        return changed
    }
    fun myName(c: Context): String = items(c).optJSONObject("name")?.takeIf { !it.isNull("v") }?.optString("v").orEmpty()
    fun setMyName(c: Context, n: String) = set(c, "name", n)

    /** The shared chats of friends, as servers that only show that one chat. */
    fun guests(c: Context): List<Machine> {
        val o = items(c); val out = ArrayList<Machine>()
        o.keys().forEach { k -> if (k.startsWith("g:")) o.getJSONObject(k).optJSONObject("v")?.let { v ->
            out += Machine((v.optString("host").ifBlank { "A friend" }) + ": " + v.optString("title"), v.getString("hostname"), v.getInt("port"), v.getString("key"), v.optBoolean("secure"), guest = true, label = v.optString("host"), share = v.getString("share"))
        } }
        return out
    }
    fun leave(c: Context, share: String) = set(c, "g:$share", null)

    private fun post(base: String, token: String, path: String, body: JSONObject?, method: String = "POST"): JSONObject {
        val c = URL(base + path).openConnection() as HttpURLConnection
        c.connectTimeout = 8_000; c.readTimeout = 15_000; c.requestMethod = method
        if (token.isNotEmpty()) c.setRequestProperty("Authorization", "Bearer $token")
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

    /** A friend uses an invite link: gets a key for that one chat and keeps it (and so on all your devices). */
    fun join(c: Context, link: String, name: String): Machine {
        val u = Uri.parse(link.trim().replaceFirst("hearth://share", "https://x/share")); val url = u.getQueryParameter("url"); val invite = u.getQueryParameter("invite")
        if (url.isNullOrBlank() || invite.isNullOrBlank()) throw ApiException(0, "That is not an invite link.")
        val r = post(url.trimEnd('/'), "", "/shares/join", JSONObject().put("invite", invite).put("name", name))
        val pu = Uri.parse(url); val secure = pu.scheme == "https"
        val g = JSONObject().put("share", r.getString("share")).put("key", r.getString("key")).put("hostname", pu.host).put("port", if (pu.port > 0) pu.port else if (secure) 443 else 80).put("secure", secure)
            .put("host", r.optString("host")).put("title", r.optString("title")).put("member", r.optString("member")).put("session", r.optString("session"))
        set(c, "g:" + r.getString("share"), g); setMyName(c, name)
        return guests(c).first { it.share == r.getString("share") }
    }

    // the host
    private fun parseShare(o: JSONObject): ShareInfo { val a = o.optJSONArray("members") ?: JSONArray(); return ShareInfo(o.getString("id"), List(a.length()) { a.getJSONObject(it).let { m -> ShareMember(m.getString("id"), m.optString("name"), m.optJSONObject("grant")?.optBoolean("auto") ?: false, m.optJSONObject("grant")?.optString("mode") ?: "plan") } }) }
    fun share(m: Machine, sid: String): ShareInfo = parseShare(post(m.base, m.token, "/sessions/$sid/share", JSONObject()))
    fun invite(m: Machine, shareId: String): String = "hearth://share?url=" + Uri.encode(m.base) + "&invite=" + post(m.base, m.token, "/shares/$shareId/invite", JSONObject()).getString("invite")
    fun setGrant(m: Machine, shareId: String, member: String, auto: Boolean, mode: String) { post(m.base, m.token, "/shares/$shareId/members/$member", JSONObject().put("grant", JSONObject().put("auto", auto).put("mode", mode))) }
    fun remove(m: Machine, shareId: String, member: String) { post(m.base, m.token, "/shares/$shareId/members/$member", null, "DELETE") }
    fun unshare(m: Machine, shareId: String) { post(m.base, m.token, "/shares/$shareId", null, "DELETE") }
    /** The host decides on a friend's request. */
    fun decide(m: Machine, req: String, allow: Boolean, text: String, mode: String, always: Boolean) { post(m.base, m.token, "/decision", JSONObject().put("req", req).put("behavior", if (allow) "allow" else "deny").put("text", text).put("mode", mode).put("always", always)) }

    // the friend
    fun guestInfo(m: Machine, sid: String): GuestInfo? {
        val c = URL(m.base + "/sessions/$sid/prefs").openConnection() as HttpURLConnection
        c.connectTimeout = 8_000; c.readTimeout = 12_000; c.setRequestProperty("Authorization", "Bearer " + m.token)
        try {
            if (c.responseCode != 200) return null
            val s = JSONObject(c.inputStream.bufferedReader().readText()).optJSONObject("shared") ?: return null
            val a = s.optJSONArray("requests") ?: JSONArray()
            return GuestInfo(s.optString("host"), s.optJSONObject("grant")?.optBoolean("auto") ?: false, List(a.length()) { a.getJSONObject(it).let { q -> GuestReq(q.getString("id"), q.optString("text"), q.optString("status"), q.optLong("ts"), q.optBoolean("edited"), q.optString("note")) } })
        } finally { c.disconnect() }
    }
}

/** Paste an invite link and your name: the shared chat appears in your list like any chat. */
@Composable
fun JoinDialog(prefill: String, onClose: () -> Unit, onJoined: (Machine) -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var link by remember { mutableStateOf(prefill) }
    var name by remember { mutableStateOf(Shares.myName(c)) }
    var err by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    AlertDialog(
        onDismissRequest = onClose,
        title = { Txt("Join a shared chat", T.h2) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Txt("You see the chat like any other. When you send something, it goes to the host to approve, and they can change it first.", T.small, muted = true)
                Field(link, { link = it.trim() }, "Paste the invite link", mono = true)
                Field(name, { name = it }, "Your name")
                err?.let { Txt(it, T.small, color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = { TextAction(if (busy) "Joining…" else "Join") {
            if (busy) return@TextAction
            busy = true; err = null
            scope.launch { runCatching { withContext(Dispatchers.IO) { Shares.join(c, link, name.trim().ifBlank { "Guest" }) } }.onSuccess { onJoined(it) }.onFailure { err = it.message; busy = false } }
        } },
        dismissButton = { TextAction("Cancel", onClose) },
    )
}

/** The host: who can open this chat, what each person may do, the invite link. */
@Composable
fun ShareDialog(m: Machine, sid: String, title: String, onClose: () -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var info by remember { mutableStateOf<ShareInfo?>(null) }
    var err by remember { mutableStateOf<String?>(null) }
    var link by remember { mutableStateOf<String?>(null) }
    fun load() { scope.launch { runCatching { withContext(Dispatchers.IO) { Shares.share(m, sid) } }.onSuccess { info = it; err = null }.onFailure { err = it.message } } }
    LaunchedEffect(sid) { load() }
    fun act(f: () -> Unit) { scope.launch { runCatching { withContext(Dispatchers.IO) { f() } }.onSuccess { load() }.onFailure { err = it.message } } }
    AlertDialog(
        onDismissRequest = onClose,
        title = { Txt("Share “$title”", T.h2) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Txt("People you invite open this chat like any other. What they send is not run at once: you get a notification, can change it, and run it on your account, or deny it. Or give someone full permission.", T.small, muted = true)
                err?.let { Txt(it, T.small, color = MaterialTheme.colorScheme.error) }
                val i = info
                if (i == null) { if (err == null) Txt("Loading…", T.small, muted = true); return@Column }
                Label("People")
                if (i.members.isEmpty()) Txt("Nobody yet. Make an invite link and send it.", T.small, muted = true)
                i.members.forEach { p ->
                    Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(14.dp)).border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(14.dp)).padding(12.dp)) {
                        Txt(p.name, T.body.copy(fontWeight = FontWeight.SemiBold))
                        Row(verticalAlignment = Alignment.CenterVertically) { Txt("Full permission: runs without asking", T.small, modifier = Modifier.weight(1f)); Switch(p.auto, { on -> act { Shares.setGrant(m, i.id, p.id, on, p.mode) } }) }
                        ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) { SHARE_MODES.forEach { (k, l) -> Chip(l, p.mode == k) { act { Shares.setGrant(m, i.id, p.id, p.auto, k) } } } }
                        TextAction("Remove ${p.name}") { act { Shares.remove(m, i.id, p.id) } }
                    }
                }
                link?.let { Txt(it, T.small.copy(fontFamily = FontFamily.Monospace)) }
                if (m.secure.not()) Txt("This chat is on a computer on your own network, so friends elsewhere cannot reach it. Share chats that live on your server.", T.small, color = MaterialTheme.colorScheme.error)
            }
        },
        confirmButton = {
            val i = info
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                if (i != null) TextAction(if (link == null) "Make invite link" else "Share link") {
                    val l = link
                    if (l == null) scope.launch { runCatching { withContext(Dispatchers.IO) { Shares.invite(m, i.id) } }.onSuccess { link = it }.onFailure { err = it.message } }
                    else c.startActivity(Intent.createChooser(Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT, l), "Invite"))
                }
                TextAction("Close", onClose)
            }
        },
        dismissButton = { if (info != null) TextAction("Stop sharing") { val i = info ?: return@TextAction; scope.launch { runCatching { withContext(Dispatchers.IO) { Shares.unshare(m, i.id) } }.onSuccess { onClose() }.onFailure { err = it.message } } } },
    )
}

/** A friend asks to use your Claude: the prompt, to change if you want, how much Claude may do, and Run it or Deny. Sits on top of the message box. */
@Composable
fun RequestDock(m: Machine, a: CallEvent, done: () -> Unit) {
    val scope = rememberCoroutineScope()
    val cs = MaterialTheme.colorScheme
    val from = a.text.substringBefore(':').ifBlank { "A friend" }
    var text by remember(a.req) { mutableStateOf(a.text.substringAfter(": ", a.text)) }
    var mode by remember(a.req) { mutableStateOf("plan") }
    var always by remember(a.req) { mutableStateOf(false) }
    var err by remember(a.req) { mutableStateOf<String?>(null) }
    fun decide(allow: Boolean) { scope.launch { runCatching { withContext(Dispatchers.IO) { Shares.decide(m, a.req, allow, text, mode, always) } }.onSuccess { done() }.onFailure { err = it.message } } }
    val shape = RoundedCornerShape(topStart = 22.dp, topEnd = 22.dp)
    Column(Modifier.fillMaxWidth().background(cs.background).padding(start = 14.dp, end = 14.dp, top = 8.dp)) {
        Column(Modifier.fillMaxWidth().clip(shape).background(cs.surfaceVariant).border(1.dp, cs.primary, shape).padding(14.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Label("$from wants to use your Claude")
            Txt("It runs in this chat, on your account. You can change the prompt first.", T.small, muted = true)
            Field(text, { text = it }, "Prompt", singleLine = false)
            ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) { SHARE_MODES.forEach { (k, l) -> Chip(l, mode == k) { mode = k } } }
            Row(verticalAlignment = Alignment.CenterVertically) { Txt("Always run $from's prompts without asking", T.small, modifier = Modifier.weight(1f)); Switch(always, { always = it }) }
            err?.let { Txt(it, T.small, color = cs.error) }
            Row(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) { Button("Run it") { decide(true) }; TextAction("Deny") { decide(false) } }
        }
    }
}
