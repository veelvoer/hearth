package dev.voer.hearth

import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.material3.Text
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL

const val SUPPORT_URL = "https://cm.vpswb.store/support"

/** Talks to the support desk. Every call returns the JSON answer or throws a message a person can read. */
object SupportApi {
    /** Set when the desk says the sign-in is over, so the screen can show the sign-in page again. */
    var expired by mutableStateOf(false)
    fun call(c: android.content.Context, method: String, path: String, body: JSONObject? = null, auth: Boolean = true): String {
        val con = URL(SUPPORT_URL + path).openConnection() as HttpURLConnection
        con.connectTimeout = 10_000; con.readTimeout = 20_000; con.requestMethod = method
        con.setRequestProperty("Content-Type", "application/json")
        if (auth) Store.supportToken(c)?.let { con.setRequestProperty("Authorization", "Bearer $it") }
        try {
            if (body != null) { con.doOutput = true; con.outputStream.use { it.write(body.toString().toByteArray()) } }
            val code = con.responseCode
            val text = (if (code in 200..299) con.inputStream else con.errorStream)?.bufferedReader()?.readText() ?: ""
            if (code == 401 && auth) { Store.setSupport(c, null, null); expired = true }
            if (code !in 200..299) throw ApiException(code, runCatching { JSONObject(text).optString("error") }.getOrNull()?.ifBlank { null } ?: "Support said $code")
            return text
        } catch (e: ApiException) { throw e } catch (e: Exception) { throw ApiException(0, "Could not reach support. Check your internet connection and try again.") } finally { con.disconnect() }
    }
}

private val CATS = listOf(
    Triple("bug", "Bug", "What happened? What did you expect? What did you do just before it went wrong?"),
    Triple("question", "Question", "What do you want to do? Where are you stuck?"),
    Triple("idea", "Idea", "What would you like Hearth to do, and why would it help you?"),
    Triple("other", "Other", "Tell us what is on your mind."),
)
private data class Ticket(val id: String, val title: String, val status: String, val last: String, val from: String, val updatedAt: Long, val email: String = "", val theirs: Boolean = false, val category: String = "other", val thread: String = "")
private data class TMsg(val from: String, val text: String, val at: Long, val files: List<String> = emptyList())
/** open / listed / answered / fixed, from what the desk says and, for older desks that do not say it, from the wording of the last automatic mail. */
private fun statusOf(status: String, from: String, last: String): String = when {
    status == "fixed" || (from == "support" && last.startsWith("Good news: what you told us about")) -> "fixed"
    status == "listed" || (from == "support" && last.startsWith("Thank you for your message about") && last.contains("added it to our list")) -> "listed"
    status == "answered" -> "answered"
    else -> "open"
}
private fun statusLabel(s: String) = when (s) { "fixed" -> "Fixed"; "listed" -> "On the list"; "answered" -> "Answered"; else -> "Open" }

/** Up to three small files that go with a message. */
@Composable
private fun Attachments(files: androidx.compose.runtime.snapshots.SnapshotStateList<Triple<String, String, String>>, onError: (String) -> Unit) {
    val c = LocalContext.current
    val pick = androidx.activity.compose.rememberLauncherForActivityResult(androidx.activity.result.contract.ActivityResultContracts.GetContent()) { uri ->
        if (uri != null) runCatching {
            if (files.size >= 3) throw ApiException(0, "You can attach up to 3 files.")
            val name = c.contentResolver.query(uri, null, null, null, null)?.use { cur -> val i = cur.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME); if (cur.moveToFirst() && i >= 0) cur.getString(i) else null } ?: "file"
            val bytes = c.contentResolver.openInputStream(uri)?.use { it.readBytes() } ?: throw ApiException(0, "Could not read that file.")
            if (bytes.size > 1_500_000) throw ApiException(0, "\"$name\" is bigger than 1.5 MB. Send a smaller file, or a screenshot.")
            files.add(Triple(name, c.contentResolver.getType(uri) ?: "application/octet-stream", android.util.Base64.encodeToString(bytes, android.util.Base64.NO_WRAP)))
        }.onFailure { onError(it.message ?: "Could not attach that file.") }
    }
    files.forEach { f -> Row(verticalAlignment = Alignment.CenterVertically) { Glyph("clip", 18.dp); Spacer(Modifier.width(8.dp)); Txt(f.first, T.small, modifier = Modifier.weight(1f), maxLines = 1); IconAction("x", "Remove ${f.first}", Alignment.CenterEnd, 40.dp) { files.remove(f) } } }
    if (files.size < 3) TextAction("Attach") { pick.launch("*/*") }
}
private fun filesJson(files: List<Triple<String, String, String>>) = org.json.JSONArray().also { a -> files.forEach { a.put(JSONObject().put("name", it.first).put("type", it.second).put("data", it.third)) } }

private fun ago(t: Long): String { val s = (System.currentTimeMillis() - t) / 1000; return when { s < 60 -> "just now"; s < 3600 -> "${s / 60} min ago"; s < 86400 -> "${s / 3600} h ago"; else -> "${s / 86400} d ago" } }

/** Settings → Support. Sign in with an emailed code, send a request, read the answers. */
@Composable
fun SupportScreen(close: () -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var email by remember { mutableStateOf(Store.supportEmail(c) ?: "") }
    var view by remember { mutableStateOf(if (Store.supportToken(c) != null) (if (Ui.supportDraft.isNotEmpty()) "new" else "home") else "login") }
    var notice by remember { mutableStateOf("") }
    var open by remember { mutableStateOf("") }
    var sentId by remember { mutableStateOf("") }
    BackHandler { if (view == "home" || view == "login") close() else view = "home" }
    LaunchedEffect(SupportApi.expired) { if (SupportApi.expired) { SupportApi.expired = false; if (view != "login") { view = "login"; notice = "You were signed out. Sign in again with a code by email." } } }
    Column(Modifier.fillMaxSize().imePadding()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) { Txt("Support", T.title) }
            if (Store.supportToken(c) != null) { TextAction("Sign out") { scope.launch(Dispatchers.IO) { runCatching { SupportApi.call(c, "POST", "/auth/logout", JSONObject()) }; Store.setSupport(c, null, null) }; view = "login" }; Spacer(Modifier.width(16.dp)) }
            TextAction("Close", close)
        }
        AnimatedContent(view, Modifier.weight(1f), label = "support") { v ->
            when (v) {
                "login" -> SupportLogin(email, notice, { email = it }) { notice = ""; view = "home" }
                "home" -> SupportHome(email, { view = "new" }) { open = it; view = "thread" }
                "new" -> SupportNew(email, { view = "home" }) { sentId = it; view = "sent" }
                "sent" -> SupportSent(sentId, email) { view = "home" }
                else -> SupportThread(open) { view = "home" }
            }
        }
    }
}

@Composable
private fun SupportLogin(email: String, notice: String, setEmail: (String) -> Unit, done: () -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var step by remember { mutableStateOf("email") }
    var code by remember { mutableStateOf("") }
    var msg by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    Screen {
        if (notice.isNotBlank()) Txt(notice, T.small, color = MaterialTheme.colorScheme.primary)
        Txt("Something not working, or an idea? Write to us here. We answer by email.", T.body, muted = true)
        Card {
            if (step == "email") {
                Txt("First, tell us your email", T.h2)
                Spacer(Modifier.height(6.dp))
                Txt("We send you a 6-digit code, so we know our answers reach you. It works with any real email address, for example Gmail. No password.", T.small, muted = true)
                Spacer(Modifier.height(10.dp))
                Field(email, { setEmail(it.trim()) }, "you@example.com")
                Spacer(Modifier.height(10.dp))
                Button(if (busy) "Sending…" else "Send me a code", Modifier.fillMaxWidth()) {
                    if (busy) return@Button
                    busy = true; msg = ""
                    scope.launch { runCatching { withContext(Dispatchers.IO) { SupportApi.call(c, "POST", "/auth/start", JSONObject().put("email", email), auth = false) } }.onSuccess { step = "code" }.onFailure { msg = it.message ?: "Something went wrong" }; busy = false }
                }
            } else {
                Txt("Check your email", T.h2)
                Spacer(Modifier.height(6.dp))
                Txt("We sent a code to $email. It can take a minute. Look in spam if you do not see it.", T.small, muted = true)
                Spacer(Modifier.height(10.dp))
                Field(code, { code = it.filter { ch -> ch.isDigit() }.take(6) }, "6-digit code", mono = true, number = true)
                Spacer(Modifier.height(10.dp))
                Button(if (busy) "Checking…" else "Sign in", Modifier.fillMaxWidth()) {
                    if (busy) return@Button
                    busy = true; msg = ""
                    scope.launch {
                        runCatching { withContext(Dispatchers.IO) { JSONObject(SupportApi.call(c, "POST", "/auth/verify", JSONObject().put("email", email).put("code", code), auth = false)) } }
                            .onSuccess { Store.setSupport(c, it.getString("token"), it.getString("email")); setEmail(it.getString("email")); done() }.onFailure { msg = it.message ?: "Something went wrong" }
                        busy = false
                    }
                }
                TextAction("Use another email") { step = "email"; code = ""; msg = "" }
            }
            if (msg.isNotBlank()) { Spacer(Modifier.height(8.dp)); Txt(msg, T.small, color = MaterialTheme.colorScheme.primary) }
        }
    }
}

@Composable
private fun SupportHome(email: String, newRequest: () -> Unit, openTicket: (String) -> Unit) {
    val c = LocalContext.current
    var tickets by remember { mutableStateOf<List<Ticket>?>(null) }
    var err by remember { mutableStateOf("") }
    var items by remember { mutableStateOf<List<CheckItem>?>(null) }   // the owner's checklist of what people reported
    var doneOpen by remember { mutableStateOf(false) }
    var checkErr by remember { mutableStateOf("") }
    val scope = rememberCoroutineScope()
    val hub = remember { Store.machines(c).let { l -> l.firstOrNull { it.secure } ?: l.firstOrNull() } }
    fun checklist(body: JSONObject) { if (hub != null) scope.launch { runCatching { withContext(Dispatchers.IO) { Shares.postJson(hub, "/inbox/checklist", body) } }.onSuccess { items = parseItems(it); checkErr = "" }.onFailure { checkErr = if ((it.message ?: "").let { m -> m.contains("unknown", true) || m.contains("404") || m.contains("not found", true) }) "Your server does not know the checklist yet. Update it in Settings, Updates, and this fills in by itself." else (it.message ?: "The checklist could not be loaded.") } } }
    LaunchedEffect(tickets) {   // looks at every report: the open ones are listed, the ones that are fixed are ticked, and the list is kept in step as long as this screen is open
        val t = tickets ?: return@LaunchedEffect
        if (hub != null && t.any { it.theirs }) checklist(JSONObject().put("sync", JSONArray().also { a -> t.filter { it.theirs }.forEach { x -> a.put(JSONObject().put("id", x.id).put("title", x.title).put("email", x.email).put("last", x.last).put("text", x.thread.ifEmpty { x.last }).put("updatedAt", x.updatedAt).put("from", x.from).put("fixed", statusOf(x.status, x.from, x.last) == "fixed")) } }))
    }
    LaunchedEffect(Unit) {
        while (true) {
            runCatching { withContext(Dispatchers.IO) { JSONArray(SupportApi.call(c, "GET", "/tickets")) } }
                .onSuccess { a -> tickets = (0 until a.length()).map { a.getJSONObject(it) }.map { Ticket(it.getString("id"), it.getString("title"), it.getString("status"), it.optString("last"), it.optString("from"), it.optLong("updatedAt"), it.optString("email"), it.optBoolean("theirs"), it.optString("category", "other"), it.optString("thread")) }; err = "" }
                .onFailure { err = it.message ?: "" }
            delay(20_000)
        }
    }
    Screen {
        Txt("Signed in as $email", T.small, muted = true)
        Button("New request", Modifier.fillMaxWidth(), newRequest)
        if (err.isNotBlank()) Txt(err, T.small, color = MaterialTheme.colorScheme.primary)
        when {
            tickets == null && err.isBlank() -> Txt("Loading…", T.small, muted = true)
            tickets.isNullOrEmpty() -> Card { Txt("No requests yet", T.h2); Spacer(Modifier.height(4.dp)); Txt("Press “New request” and tell us what is going on. We answer by email, and you can read the answer here too.", T.small, muted = true) }
            else -> {
                val order = listOf("open", "listed", "answered", "fixed")
                val isOwner = tickets!!.any { it.theirs } && hub != null
                if (isOwner) OwnerChecklist(items, checkErr, tickets!!.filter { it.theirs && statusOf(it.status, it.from, it.last) != "fixed" }, doneOpen, { doneOpen = !doneOpen }, { id, d -> checklist(JSONObject().put("id", id).put("done", d)) }, { checklist(JSONObject().put("remove", "done")) }, { id -> openTicket(id) }, hub!!)
                val mine = tickets!!.filter { !(isOwner && it.theirs) }
                val groups = mine.groupBy { statusOf(it.status, it.from, it.last) }
                if (isOwner && mine.isNotEmpty()) Label("Your own requests")
                order.forEach { st ->
                    val list = groups[st] ?: return@forEach
                    if (groups.size > 1) Label(statusLabel(st))
                    list.forEach { t ->
                        Card(Modifier.pressable { openTicket(t.id) }) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Txt(t.title, T.h2, modifier = Modifier.weight(1f))
                                Spacer(Modifier.width(8.dp))
                                if (st == "fixed") { Glyph("check", 16.dp, MaterialTheme.colorScheme.primary); Spacer(Modifier.width(4.dp)) }
                                Txt(statusLabel(st), T.small, color = if (st == "fixed" || st == "answered") MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.primary)
                            }
                            Spacer(Modifier.height(4.dp))
                            Txt((if (t.theirs) t.email + " · " else "") + (if (t.from == "support") "Hearth Support: " else if (t.theirs) "They: " else "You: ") + t.last, T.small, muted = true, maxLines = 3)
                            Txt("${t.id} · ${ago(t.updatedAt)}", T.small, muted = true)
                            if (t.theirs && st != "fixed") SupportActions(t.id, t.title, t.category, t.email, t.thread.ifEmpty { t.last })
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun SupportNew(email: String, back: () -> Unit, sent: (String) -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var cat by remember { mutableStateOf("bug") }
    val draft = remember { Ui.supportDraft.also { Ui.supportDraft = "" } }
    var title by remember { mutableStateOf(if (draft.isNotEmpty()) "Something went wrong in a chat" else "") }
    var text by remember { mutableStateOf(draft) }
    var info by remember { mutableStateOf(true) }
    var msg by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    val files = remember { androidx.compose.runtime.mutableStateListOf<Triple<String, String, String>>() }
    Screen {
        BackLink("Back", back)
        Txt("What is it about?", T.small, muted = true)
        ChipRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) { CATS.forEach { (k, l, _) -> Chip(l, cat == k) { cat = k } } }
        Field(title, { title = it.take(120) }, "A few words, e.g. “The app closes when I open it”")
        Field(text, { text = it.take(5000) }, CATS.first { it.first == cat }.third, singleLine = false, modifier = Modifier.heightIn(min = 140.dp))
        Attachments(files) { msg = it }
        Row(verticalAlignment = Alignment.CenterVertically) {
            androidx.compose.material3.Checkbox(info, { info = it })
            Txt("Include app info (version and system), so we can find the problem faster", T.small, modifier = Modifier.weight(1f))
        }
        Txt("We use your email only to answer you. Never put passwords or secret keys in your message.", T.small, muted = true)
        Button(if (busy) "Sending…" else "Send request", Modifier.fillMaxWidth()) {
            if (busy) return@Button
            busy = true; msg = ""
            val meta = JSONObject().apply { if (info) put("app", "Phone").put("version", Updates.current(c)).put("platform", "Android ${Build.VERSION.RELEASE} · ${Build.MANUFACTURER} ${Build.MODEL}") }
            scope.launch {
                runCatching { withContext(Dispatchers.IO) { JSONObject(SupportApi.call(c, "POST", "/tickets", JSONObject().put("category", cat).put("title", title).put("text", text).put("meta", meta).put("files", filesJson(files)))) } }
                    .onSuccess { sent(it.getString("id")) }.onFailure { msg = it.message ?: "Something went wrong" }
                busy = false
            }
        }
        if (msg.isNotBlank()) Txt(msg, T.small, color = MaterialTheme.colorScheme.primary)
    }
}

@Composable
private fun SupportSent(id: String, email: String, done: () -> Unit) = Screen {
    Card {
        Txt("✓ Request sent", T.h2)
        Spacer(Modifier.height(6.dp))
        Txt("Your number is $id.", T.body)
        Spacer(Modifier.height(4.dp))
        Txt("We just emailed you a confirmation to $email. When we answer, the answer arrives in your email, and you can read it here too.", T.small, muted = true)
    }
    Button("Done", Modifier.fillMaxWidth(), done)
}

@Composable
private fun SupportThread(id: String, back: () -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var title by remember { mutableStateOf("") }
    var msgs by remember { mutableStateOf<List<TMsg>>(emptyList()) }
    var reply by remember { mutableStateOf("") }
    var err by remember { mutableStateOf("") }
    val rfiles = remember { androidx.compose.runtime.mutableStateListOf<Triple<String, String, String>>() }
    var status by remember { mutableStateOf("") }
    var theirs by remember { mutableStateOf("") }   // the sender, when this is somebody else's request that you see as owner
    suspend fun load() {
        runCatching { withContext(Dispatchers.IO) { JSONObject(SupportApi.call(c, "GET", "/tickets/$id")) } }
            .onSuccess { o -> title = o.getString("title"); theirs = if (o.optBoolean("theirs")) o.optString("email") else ""; val a = o.getJSONArray("messages"); msgs = (0 until a.length()).map { a.getJSONObject(it) }.map { TMsg(it.getString("from"), it.getString("text"), it.optLong("at"), it.optJSONArray("files")?.let { f -> (0 until f.length()).map { i -> f.getJSONObject(i).optString("name") } } ?: emptyList()) }; status = o.optString("status"); err = "" }
            .onFailure { err = it.message ?: "" }
    }
    LaunchedEffect(id) { while (true) { load(); delay(15_000) } }
    Screen {
        BackLink("All requests", back)
        Txt(title, T.h2)
        Txt(id + (msgs.lastOrNull()?.let { " · " + statusLabel(statusOf(status, it.from, it.text)) } ?: ""), T.small, muted = true)
        if (theirs.isNotEmpty()) { Txt("From $theirs. You see everything here and cannot change it. Answer by replying to the email.", T.small, muted = true); SupportActions(id, title, "other", theirs, msgs.joinToString("\n\n") { (if (it.from == "support") "Support" else theirs) + ": " + it.text }) }
        msgs.forEach { m ->
            Card {
                Txt((if (m.from == "support") "Hearth Support" else if (theirs.isNotEmpty()) theirs else "You") + " · " + ago(m.at), T.small, muted = true)
                Spacer(Modifier.height(4.dp))
                Txt(m.text, T.body, color = if (m.from == "support") MaterialTheme.colorScheme.primary else androidx.compose.ui.graphics.Color.Unspecified)
                m.files.forEach { n -> Row(verticalAlignment = Alignment.CenterVertically) { Glyph("clip", 16.dp); Spacer(Modifier.width(6.dp)); Txt(n, T.small, muted = true) } }
            }
        }
        if (theirs.isEmpty()) Field(reply, { reply = it.take(5000) }, "Write a reply…", singleLine = false, modifier = Modifier.heightIn(min = 90.dp))
        if (theirs.isEmpty()) Attachments(rfiles) { err = it }
        if (theirs.isEmpty()) Button("Send", Modifier.fillMaxWidth()) {
            if (reply.isBlank()) return@Button
            scope.launch { runCatching { withContext(Dispatchers.IO) { SupportApi.call(c, "POST", "/tickets/$id/messages", JSONObject().put("text", reply).put("files", filesJson(rfiles))) } }.onSuccess { reply = ""; rfiles.clear(); load() }.onFailure { err = it.message ?: "" } }
        }
        if (err.isNotBlank()) Txt(err, T.small, color = MaterialTheme.colorScheme.primary)
    }
}

private data class CheckItem(val id: String, val title: String, val from: String, val text: String, val at: Long, val done: Boolean)
private fun parseItems(o: JSONObject): List<CheckItem> = o.optJSONArray("items")?.let { a -> (0 until a.length()).map { a.getJSONObject(it) }.map { CheckItem(it.getString("id"), it.optString("title"), it.optString("from"), it.optString("text"), it.optLong("at"), it.optBoolean("done")) } } ?: emptyList()

/** What people reported, as a checklist: tick it when it is done; done ones go to the bottom and can be removed. */
@Composable
private fun OwnerChecklist(items: List<CheckItem>?, checkErr: String, fallback: List<Ticket>, doneOpen: Boolean, toggleDone: () -> Unit, tick: (String, Boolean) -> Unit, removeDone: () -> Unit, open: (String) -> Unit, hub: Machine) {
    val cs = MaterialTheme.colorScheme
    val todo = items?.filter { !it.done }.orEmpty(); val done = items?.filter { it.done }.orEmpty()
    Label("To do · ${todo.size}")
    if (items == null && checkErr.isNotEmpty()) {   // the server could not give the checklist: say why, and still show the open reports so nothing is hidden
        Card { Txt(checkErr, T.small, color = cs.primary) }
        fallback.forEach { t -> Card(Modifier.pressable { open(t.id) }) { Txt(t.title, T.h2); Txt(t.email + " · " + t.id + " · " + ago(t.updatedAt), T.small, muted = true); SupportActions(t.id, t.title, t.category, t.email, t.thread.ifEmpty { t.last }, hub) } }
    }
    else if (items == null) Txt("Loading…", T.small, muted = true)
    else if (todo.isEmpty()) Card { Txt("Nothing left to do", T.h2); Txt("Every report is done. New ones show up here by themselves.", T.small, muted = true) }
    else Card {
        todo.forEachIndexed { i, x ->
            if (i > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(cs.outline))
            CheckRow(x, tick, open)
            SupportActions(x.id, x.title, "other", x.from, x.text, hub)
        }
    }
    if (done.isNotEmpty()) {
        Row(Modifier.fillMaxWidth().clickable(onClick = toggleDone, role = androidx.compose.ui.semantics.Role.Button).heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically) {
            Label("Done · ${done.size}", Modifier.weight(1f)); Glyph(if (doneOpen) "chevu" else "chevd", 20.dp)
        }
        if (doneOpen) Card {
            done.forEach { x -> CheckRow(x, tick, open) }
            androidx.compose.runtime.CompositionLocalProvider(LocalIconAlign provides Alignment.CenterStart) { TextAction("Remove done") { removeDone() } }
        }
    }
}

@Composable
private fun CheckRow(x: CheckItem, tick: (String, Boolean) -> Unit, open: (String) -> Unit) {
    val cs = MaterialTheme.colorScheme
    Row(verticalAlignment = Alignment.CenterVertically) {
        androidx.compose.material3.Checkbox(x.done, { tick(x.id, it) })
        Column(Modifier.weight(1f).clickable(role = androidx.compose.ui.semantics.Role.Button) { open(x.id) }.padding(vertical = 8.dp)) {
            Text(x.title, style = T.body.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold, textDecoration = if (x.done) androidx.compose.ui.text.style.TextDecoration.LineThrough else null), color = if (x.done) cs.onSurfaceVariant else cs.onSurface, maxLines = 2)
            Txt(x.from + " · " + x.id + " · " + ago(x.at), T.small, muted = true, maxLines = 1)
        }
    }
}

/** For an owner: what to do with somebody's request. The same choices as the notification. */
@Composable
private fun SupportActions(id: String, title: String, category: String, from: String, text: String, hub: Machine? = null) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var msg by remember(id) { mutableStateOf("") }
    fun act(action: String, label: String) {
        val m = hub ?: Store.machines(c).let { l -> l.firstOrNull { it.secure } ?: l.firstOrNull() }
        if (m == null) { msg = "Connect your server first."; return }
        msg = "$label…"
        scope.launch {
            runCatching { withContext(Dispatchers.IO) { Shares.postJson(m, "/inbox/act", JSONObject().put("id", id).put("title", title).put("category", category).put("from", from).put("text", text.take(5000)).put("action", action)) } }
                .onSuccess { msg = it.optString("text", "Done."); val sid = it.optString("session"); if (action == "claude" && it.optBoolean("ok", true) && sid.isNotEmpty()) { Ui.pendingOpen = PendingOpen(m.host, sid, title, it.optString("cwd"), false); Ui.tab = 0 } }.onFailure { msg = if ((it.message ?: "").contains("unknown", true)) "Update your server first." else it.message ?: "Didn't work" }
        }
    }
    Spacer(Modifier.height(8.dp))
    Row(horizontalArrangement = Arrangement.spacedBy(16.dp), verticalAlignment = Alignment.CenterVertically) {
        if (hub == null) TextAction("Add to tasks") { act("list", "Adding") }
        TextAction("Fix with Claude") { act("claude", "Starting Claude") }
    }
    if (msg.isNotEmpty()) Txt(msg, T.small, muted = true)
}
