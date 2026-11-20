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
data class Chat(val id: String, val title: String, val busy: Boolean, val mtime: Long, val server: Server, val pinned: Boolean = false)
data class Ask(val req: String, val kind: String, val title: String, val text: String, val options: List<String>, val server: Server)
data class Home(val usage: Usage?, val chats: List<Chat>, val asks: List<Ask>)
/** One chat's last turn: what you asked, Claude's newest words, its last steps ("Read src/Api.kt"), and whether the newest thing is a step (it is doing that now). */
data class Turn(val asked: String, val answer: String, val steps: List<String> = emptyList(), val ts: Long = 0, val now: String = "")

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
        return parseUsage(o)
    }

    private fun parseUsage(o: JSONObject): Usage {
        fun pct(k: String) = o.optJSONObject(k)?.optDouble("pct")?.takeIf { !it.isNaN() }
        return Usage(pct("session"), pct("week"), o.optJSONObject("session")?.let { if (it.isNull("resetsAt")) null else it.optLong("resetsAt") })
    }

    private fun parseAsks(a: JSONArray, s: Server): List<Ask> =
        (0 until a.length()).map { a.getJSONObject(it) }.filter { it.optString("kind") == "permission" || it.optString("kind") == "question" }.filter { it.optString("req").isNotEmpty() }
            .map { o -> Ask(o.optString("req"), o.optString("kind"), o.optString("title"), o.optString("text"), o.optJSONArray("options")?.let { op -> (0 until op.length()).map { i -> op.optJSONObject(i)?.optString("label") ?: op.optString(i) } } ?: emptyList(), s) }

    private fun parseChats(a: JSONArray, s: Server): List<Chat> =
        (0 until a.length()).map { a.getJSONObject(it) }.filter { it.optString("kind") != "talk" || it.optString("title").isNotBlank() }
            .map { Chat(it.getString("id"), it.optString("title").ifBlank { "Chat" }, it.optBoolean("running") || it.optBoolean("busy"), it.optLong("mtime"), s, it.optBoolean("pin")) }

    /** Straight to the server (no phone): the pins and names of your chats, the same as on the phone, pinned ones first. */
    private fun withTags(s: Server, l: List<Chat>): List<Chat> {
        val t = runCatching { JSONObject(get(s, "/chat-tags")).optJSONObject("tags") }.getOrNull() ?: return l
        return l.map { c -> t.optJSONObject(c.id)?.let { o -> c.copy(title = o.optString("name").ifBlank { c.title }, pinned = o.optBoolean("pin")) } ?: c }
            .sortedWith(compareByDescending<Chat> { it.pinned }.thenByDescending { it.busy }.thenByDescending { it.mtime })
    }

    /** The raw answer of the last good home call per server, for a screen that appears at once and is then brought up to date. */
    private val last = java.util.concurrent.ConcurrentHashMap<String, Home>()
    private fun prefs() = Phone.app.getSharedPreferences("cache", Context.MODE_PRIVATE)

    /** The last numbers and chats (also from the last time the app ran): drawn at once, then the fresh ones replace them. */
    fun cached(s: Server): Home? = last[s.base] ?: runCatching {
        val o = JSONObject(prefs().getString(s.base, null) ?: return null)
        val u = o.optJSONObject("u")?.let { Usage(it.optDouble("s").takeIf { v -> !v.isNaN() }, it.optDouble("w").takeIf { v -> !v.isNaN() }, if (it.has("r")) it.optLong("r") else null) }
        val a = o.optJSONArray("c") ?: JSONArray()
        Home(u, (0 until a.length()).map { a.getJSONObject(it) }.map { Chat(it.getString("id"), it.optString("t"), it.optBoolean("b"), it.optLong("m"), s, it.optBoolean("p")) }, emptyList())
    }.getOrNull()

    private val lastAt = java.util.concurrent.ConcurrentHashMap<String, Long>()

    /** For the watch face: one fetch serves every complication that asks within [maxAge] (they are updated together), so the face costs one Bluetooth message per round. */
    fun homeRecent(s: Server, maxAge: Long = 60_000): Home {
        val at = lastAt[s.base] ?: 0L
        last[s.base]?.let { if (System.currentTimeMillis() - at < maxAge) return it }
        synchronized(this) {
            last[s.base]?.let { if (System.currentTimeMillis() - (lastAt[s.base] ?: 0L) < maxAge) return it }
            return home(s)
        }
    }

    private fun remember(s: Server, h: Home) {
        last[s.base] = h; lastAt[s.base] = System.currentTimeMillis()
        runCatching {
            val u = h.usage?.let { JSONObject().apply { it.session?.let { v -> put("s", v) }; it.week?.let { v -> put("w", v) }; it.sessionReset?.let { v -> put("r", v) } } }
            val c = JSONArray().also { a -> h.chats.take(12).forEach { ch -> a.put(JSONObject().put("id", ch.id).put("t", ch.title).put("b", ch.busy).put("m", ch.mtime).put("p", ch.pinned)) } }
            prefs().edit().putString(s.base, JSONObject().apply { if (u != null) put("u", u); put("c", c) }.toString()).apply()
        }
    }

    /** Usage, chats and what Claude waits for, in one go: one message to the phone (which fetches them side by side), or three calls side by side. */
    fun home(s: Server): Home {
        val raw = try { Phone.ask(s, "home") } catch (e: IOException) { if (e.message == "unknown") null else throw e }
        if (raw != null) {
            if (!raw.has("usage") && !raw.has("chats") && !raw.has("items")) throw IOException(raw.optString("error", "no answer"))
            return Home(raw.optJSONObject("usage")?.let { parseUsage(it) }, parseChats(raw.optJSONArray("chats") ?: JSONArray(), s), parseAsks(raw.optJSONArray("items") ?: JSONArray(), s)).also { remember(s, it) }
        }
        val r = arrayOfNulls<Result<Any?>>(3)
        listOf<() -> Any?>({ usage(s) }, { chats(s) }, { pending(s) }).mapIndexed { i, f -> Thread { r[i] = runCatching { f() } }.also { it.start() } }.forEach { it.join() }
        if (r.all { it?.isFailure != false }) throw r.firstNotNullOf { it?.exceptionOrNull() }
        @Suppress("UNCHECKED_CAST")
        return Home(r[0]?.getOrNull() as? Usage, (r[1]?.getOrNull() as? List<Chat>) ?: emptyList(), (r[2]?.getOrNull() as? List<Ask>) ?: emptyList()).also { remember(s, it) }
    }

    /** What Claude is waiting for: permissions and questions. */
    fun pending(s: Server): List<Ask> {
        val a = Phone.ask(s, "pending")?.getJSONArray("items") ?: JSONArray(get(s, "/pending")).also { Phone.via = false }
        return parseAsks(a, s)
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
        Phone.ask(s, "chats")?.getJSONArray("chats")?.let { return parseChats(it, s) }
        Phone.via = false
        return withTags(s, parseChats(JSONArray(get(s, "/sessions")), s))
    }

    /** The last thing you asked and the last answer, without tool calls. */
    fun lastTurn(s: Server, id: String): Turn {
        Phone.ask(s, "turn") { put("id", id) }?.let { o -> val st = o.optJSONArray("steps") ?: JSONArray(); return Turn(o.optString("asked"), o.optString("answer"), (0 until st.length()).map { st.optString(it) }, o.optLong("ts"), o.optString("now")) }
        Phone.via = false
        val a = JSONArray(get(s, "/sessions/$id/messages?tail=40"))
        var asked = ""; var answer = ""; val steps = ArrayList<String>(); var ts = 0L; var now = ""
        for (i in a.length() - 1 downTo 0) {
            val o = a.getJSONObject(i)
            if (ts == 0L) ts = o.optLong("ts")
            if (now.isEmpty() && o.optString("role") != "meta") now = o.optString("role")
            when (o.optString("role")) {
                "assistant" -> if (answer.isEmpty()) answer = o.optString("text")
                "tool" -> if (steps.size < 4) steps.add(0, o.optString("text").take(90))
                "user" -> asked = o.optString("text")
            }
            if (asked.isNotEmpty()) break
        }
        return Turn(asked, answer, steps, ts, now)
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
