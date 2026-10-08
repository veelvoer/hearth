package dev.voer.hearth

import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
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
    fun call(c: android.content.Context, method: String, path: String, body: JSONObject? = null, auth: Boolean = true): String {
        val con = URL(SUPPORT_URL + path).openConnection() as HttpURLConnection
        con.connectTimeout = 10_000; con.readTimeout = 20_000; con.requestMethod = method
        con.setRequestProperty("Content-Type", "application/json")
        if (auth) Store.supportToken(c)?.let { con.setRequestProperty("Authorization", "Bearer $it") }
        try {
            if (body != null) { con.doOutput = true; con.outputStream.use { it.write(body.toString().toByteArray()) } }
            val code = con.responseCode
            val text = (if (code in 200..299) con.inputStream else con.errorStream)?.bufferedReader()?.readText() ?: ""
            if (code == 401 && auth) Store.setSupport(c, null, null)
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
private data class Ticket(val id: String, val title: String, val status: String, val last: String, val from: String, val updatedAt: Long)
private data class TMsg(val from: String, val text: String, val at: Long)
private fun ago(t: Long): String { val s = (System.currentTimeMillis() - t) / 1000; return when { s < 60 -> "just now"; s < 3600 -> "${s / 60} min ago"; s < 86400 -> "${s / 3600} h ago"; else -> "${s / 86400} d ago" } }

/** Settings → Support. Sign in with an emailed code, send a request, read the answers. */
@Composable
fun SupportScreen(close: () -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var email by remember { mutableStateOf(Store.supportEmail(c) ?: "") }
    var view by remember { mutableStateOf(if (Store.supportToken(c) != null) "home" else "login") }
    var open by remember { mutableStateOf("") }
    var sentId by remember { mutableStateOf("") }
    BackHandler { if (view == "home" || view == "login") close() else view = "home" }
    Column(Modifier.fillMaxSize()) {
        Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 14.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) { Txt("Support", T.title) }
            if (Store.supportToken(c) != null) { TextAction("Sign out") { scope.launch(Dispatchers.IO) { runCatching { SupportApi.call(c, "POST", "/auth/logout", JSONObject()) }; Store.setSupport(c, null, null) }; view = "login" }; Spacer(Modifier.width(16.dp)) }
            TextAction("Close", close)
        }
        AnimatedContent(view, Modifier.weight(1f), label = "support") { v ->
            when (v) {
                "login" -> SupportLogin(email, { email = it }) { view = "home" }
                "home" -> SupportHome(email, { view = "new" }) { open = it; view = "thread" }
                "new" -> SupportNew(email, { view = "home" }) { sentId = it; view = "sent" }
                "sent" -> SupportSent(sentId, email) { view = "home" }
                else -> SupportThread(open) { view = "home" }
            }
        }
    }
}

@Composable
private fun SupportLogin(email: String, setEmail: (String) -> Unit, done: () -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var step by remember { mutableStateOf("email") }
    var code by remember { mutableStateOf("") }
    var msg by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    Screen {
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
    LaunchedEffect(Unit) {
        while (true) {
            runCatching { withContext(Dispatchers.IO) { JSONArray(SupportApi.call(c, "GET", "/tickets")) } }
                .onSuccess { a -> tickets = (0 until a.length()).map { a.getJSONObject(it) }.map { Ticket(it.getString("id"), it.getString("title"), it.getString("status"), it.optString("last"), it.optString("from"), it.optLong("updatedAt")) }; err = "" }
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
            else -> tickets!!.forEach { t ->
                Card(Modifier.pressable { openTicket(t.id) }) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Txt(t.title, T.h2, modifier = Modifier.weight(1f))
                        Txt(if (t.status == "answered") "Answered" else "Open", T.small, color = MaterialTheme.colorScheme.primary)
                    }
                    Spacer(Modifier.height(4.dp))
                    Txt((if (t.from == "support") "Hearth Support: " else "You: ") + t.last, T.small, muted = true)
                    Txt("${t.id} · ${ago(t.updatedAt)}", T.small, muted = true)
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
    var title by remember { mutableStateOf("") }
    var text by remember { mutableStateOf("") }
    var info by remember { mutableStateOf(true) }
    var msg by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    Screen {
        TextAction("‹ Back", back)
        Txt("What is it about?", T.small, muted = true)
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) { CATS.forEach { (k, l, _) -> Chip(l, cat == k) { cat = k } } }
        Field(title, { title = it.take(120) }, "A few words, e.g. “The app closes when I open it”")
        Field(text, { text = it.take(5000) }, CATS.first { it.first == cat }.third, singleLine = false, modifier = Modifier.heightIn(min = 140.dp))
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
                runCatching { withContext(Dispatchers.IO) { JSONObject(SupportApi.call(c, "POST", "/tickets", JSONObject().put("category", cat).put("title", title).put("text", text).put("meta", meta))) } }
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
    suspend fun load() {
        runCatching { withContext(Dispatchers.IO) { JSONObject(SupportApi.call(c, "GET", "/tickets/$id")) } }
            .onSuccess { o -> title = o.getString("title"); val a = o.getJSONArray("messages"); msgs = (0 until a.length()).map { a.getJSONObject(it) }.map { TMsg(it.getString("from"), it.getString("text"), it.optLong("at")) }; err = "" }
            .onFailure { err = it.message ?: "" }
    }
    LaunchedEffect(id) { while (true) { load(); delay(15_000) } }
    Screen {
        TextAction("‹ All requests", back)
        Txt(title, T.h2)
        Txt(id, T.small, muted = true)
        msgs.forEach { m ->
            Card {
                Txt((if (m.from == "support") "Hearth Support" else "You") + " · " + ago(m.at), T.small, muted = true)
                Spacer(Modifier.height(4.dp))
                Txt(m.text, T.body, color = if (m.from == "support") MaterialTheme.colorScheme.primary else androidx.compose.ui.graphics.Color.Unspecified)
            }
        }
        Field(reply, { reply = it.take(5000) }, "Write a reply…", singleLine = false, modifier = Modifier.heightIn(min = 90.dp))
        Button("Send", Modifier.fillMaxWidth()) {
            if (reply.isBlank()) return@Button
            scope.launch { runCatching { withContext(Dispatchers.IO) { SupportApi.call(c, "POST", "/tickets/$id/messages", JSONObject().put("text", reply)) } }.onSuccess { reply = ""; load() }.onFailure { err = it.message ?: "" } }
        }
        if (err.isNotBlank()) Txt(err, T.small, color = MaterialTheme.colorScheme.primary)
    }
}
