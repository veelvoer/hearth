package dev.voer.hearth.wear

import android.content.Context
import com.google.android.gms.tasks.Tasks
import com.google.android.gms.wearable.CapabilityClient
import com.google.android.gms.wearable.Wearable
import java.io.IOException
import java.util.concurrent.TimeUnit
import java.net.HttpURLConnection
import java.net.URL
import org.json.JSONArray
import org.json.JSONObject

data class Usage(val session: Double?, val week: Double?, val sessionReset: Long?)
data class Chat(val id: String, val title: String, val busy: Boolean, val mtime: Long, val server: Server)
data class Ask(val req: String, val kind: String, val title: String, val text: String, val options: List<String>, val server: Server)
data class Turn(val asked: String, val answer: String)

/**
 * The phone, over Bluetooth: the watch asks the Hearth phone app and the phone makes the call to your server.
 * That keeps the watch's Wi-Fi off, which is where its battery went. Only when no phone with Hearth is in reach does
 * the watch call the server itself.
 */
object Phone {
    const val PATH = "/hearth/rpc"
    private const val CAPABILITY = "hearth_phone_link"
    lateinit var app: Context
    @Volatile private var node: String? = null
    @Volatile private var checkedAt = 0L
    /** How the last call went: true = through the phone, false = straight to the server, null = not yet. */
    @Volatile var via: Boolean? = null

    private fun node(): String? {
        if (!::app.isInitialized) return null
        if (System.currentTimeMillis() - checkedAt < 30_000) return node
        checkedAt = System.currentTimeMillis()
        node = runCatching {
            val cap = Tasks.await(Wearable.getCapabilityClient(app).getCapability(CAPABILITY, CapabilityClient.FILTER_REACHABLE), 4, TimeUnit.SECONDS)
            cap.nodes.firstOrNull { it.isNearby }?.id ?: cap.nodes.firstOrNull()?.id
        }.getOrNull()
        return node
    }

    /** Asks the phone; null when there is no phone to ask (then the caller goes straight to the server). */
    fun ask(s: Server, op: String, extra: JSONObject.() -> Unit = {}): JSONObject? {
        val n = node() ?: return null
        val q = JSONObject().put("op", op).put("base", s.base).apply(extra)
        val r = runCatching { Tasks.await(Wearable.getMessageClient(app).sendRequest(n, PATH, q.toString().toByteArray()), 30, TimeUnit.SECONDS) }
            .getOrElse { checkedAt = 0; return null }   // the phone went out of reach: look again next time
        via = true
        val o = JSONObject(String(r))
        if (o.has("error")) throw IOException(if (o.optInt("code") == 401) "The phone needs to connect again" else if (o.has("code")) "${s.name} ${o.getString("error")}" else o.getString("error"))
        return o
    }
}

/** The few calls the watch needs: through the phone over Bluetooth, or (no phone in reach) over the watch's own connection. */
object Api {
    private fun open(s: Server, path: String, post: Boolean = false, read: Int = 15_000) = (URL(s.base + path).openConnection() as HttpURLConnection).apply {
        connectTimeout = 8_000; readTimeout = read
        setRequestProperty("Authorization", "Bearer ${s.token}")
        if (post) { requestMethod = "POST"; doOutput = true; setRequestProperty("Content-Type", "application/json") }
    }

    private fun get(s: Server, path: String): String {
        val c = open(s, path)
        try {
            if (c.responseCode == 401) throw IOException("The phone needs to connect again")
            if (c.responseCode != 200) throw IOException("${s.name} said ${c.responseCode}")
            return c.inputStream.bufferedReader().readText()
        } finally { c.disconnect() }
    }

    private fun post(s: Server, path: String, body: JSONObject, read: Int = 15_000): Int {
        val c = open(s, path, post = true, read = read)
        try {
            c.outputStream.use { it.write(body.toString().toByteArray()) }
            val code = c.responseCode
            if (code == 200) c.inputStream.bufferedReader().use { r -> repeat(2) { r.readLine() ?: return code } }   // the run is started: no need to stay for the answer, the server keeps going
            return code
        } finally { c.disconnect() }
    }

    fun usage(s: Server): Usage {
        val o = Phone.ask(s, "usage")?.getJSONObject("usage") ?: JSONObject(get(s, "/usage")).also { Phone.via = false }
        fun pct(k: String) = o.optJSONObject(k)?.optDouble("pct")?.takeIf { !it.isNaN() }
        return Usage(pct("session"), pct("week"), o.optJSONObject("session")?.let { if (it.isNull("resetsAt")) null else it.optLong("resetsAt") })
    }

    /** What Claude is waiting for: permissions and questions. */
    fun pending(s: Server): List<Ask> {
        val a = Phone.ask(s, "pending")?.getJSONArray("items") ?: JSONArray(get(s, "/pending")).also { Phone.via = false }
        return (0 until a.length()).map { a.getJSONObject(it) }.filter { it.optString("kind") == "permission" || it.optString("kind") == "question" }.filter { it.optString("req").isNotEmpty() }
            .map { o -> Ask(o.optString("req"), o.optString("kind"), o.optString("title"), o.optString("text"), o.optJSONArray("options")?.let { op -> (0 until op.length()).map { i -> op.optJSONObject(i)?.optString("label") ?: op.optString(i) } } ?: emptyList(), s) }
    }
    /** Allow or deny a permission, or pick an answer to a question. */
    fun decide(s: Server, req: String, kind: String, v: String) {
        if (Phone.ask(s, "decide") { put("req", req); put("kind", kind); put("v", v) } != null) return
        if (kind == "permission") post(s, "/decision", JSONObject().put("req", req).put("behavior", v)) else post(s, "/answer", JSONObject().put("req", req).put("answer", v))
    }

    /** The look of the computer (colors), or null when it has none to share. */
    fun theme(s: Server): JSONObject? {
        val o = Phone.ask(s, "theme") ?: JSONObject(get(s, "/theme")).also { Phone.via = false }
        return o.optJSONObject("theme")
    }

    fun chats(s: Server): List<Chat> {
        val a = Phone.ask(s, "chats")?.getJSONArray("chats") ?: JSONArray(get(s, "/sessions")).also { Phone.via = false }
        return (0 until a.length()).map { a.getJSONObject(it) }.filter { it.optString("kind") != "talk" || it.optString("title").isNotBlank() }
            .map { Chat(it.getString("id"), it.optString("title").ifBlank { "Chat" }, it.optBoolean("running") || it.optBoolean("busy"), it.optLong("mtime"), s) }
    }

    /** The last thing you asked and the last answer, without tool calls. */
    fun lastTurn(s: Server, id: String): Turn {
        Phone.ask(s, "turn") { put("id", id) }?.let { return Turn(it.optString("asked"), it.optString("answer")) }
        Phone.via = false
        val a = JSONArray(get(s, "/sessions/$id/messages?tail=30"))
        var asked = ""; var answer = ""
        for (i in a.length() - 1 downTo 0) {
            val o = a.getJSONObject(i)
            when (o.optString("role")) {
                "assistant" -> if (answer.isEmpty() && asked.isEmpty()) answer = o.optString("text")
                "user" -> if (asked.isEmpty()) asked = o.optString("text")
            }
            if (asked.isNotEmpty()) break
        }
        return Turn(asked, answer)
    }

    fun send(s: Server, id: String, text: String) {
        val code = Phone.ask(s, "send") { put("id", id); put("text", text) }?.optInt("code")
            ?: post(s, "/sessions/$id/send", JSONObject().put("text", text).put("mode", "acceptEdits"), read = 20_000)
        if (code == 409) throw IOException("Claude is still busy with this chat")
        if (code != 200) throw IOException("${s.name} said $code")
    }

    fun stop(s: Server, id: String) { if (Phone.ask(s, "stop") { put("id", id) } == null) post(s, "/stop", JSONObject().put("session", id)) }
}

/** Claude's markdown, made readable on a small round screen. */
fun plain(md: String): String = md.replace("**", "").replace("`", "").lines().joinToString("\n") { l ->
    val t = l.trimStart(); when { t.startsWith("#") -> t.trimStart('#').trim(); t.startsWith("- ") || t.startsWith("* ") -> "• " + t.drop(2); else -> l }
}.replace(Regex("\n{3,}"), "\n\n").trim()
