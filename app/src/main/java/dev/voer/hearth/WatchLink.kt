package dev.voer.hearth

import com.google.android.gms.tasks.Task
import com.google.android.gms.tasks.Tasks
import com.google.android.gms.wearable.WearableListenerService
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors
import org.json.JSONArray
import org.json.JSONObject

/**
 * The watch's way to your servers: it asks this phone over Bluetooth (the Wear OS data layer) and the phone makes the
 * call on its own connection. The watch then never has to turn on its Wi-Fi, which is what drained its battery.
 * Answers are small and already shaped for the watch (a message over the data layer must stay under 100 KB).
 * Only servers this phone knows are reached, with this phone's own keys.
 */
class WatchLink : WearableListenerService() {
    private val pool = Executors.newFixedThreadPool(3)
    private val fan = Executors.newCachedThreadPool()   // the side-by-side calls of one request (never the same pool: they would wait for each other)

    override fun onRequest(nodeId: String, path: String, request: ByteArray): Task<ByteArray>? {
        if (path != PATH) return null
        return Tasks.call(pool) { runCatching { answer(JSONObject(String(request))) }.getOrElse { JSONObject().put("error", it.message ?: "failed") }.toString().toByteArray() }
    }

    private fun answer(q: JSONObject): JSONObject {
        val m = Store.machines(this).firstOrNull { !it.guest && it.base.trimEnd('/') == q.optString("base").trimEnd('/') }
            ?: return JSONObject().put("error", "This phone does not know that server any more")
        val id = q.optString("id")
        if (id.isNotEmpty() && !Regex("^[A-Za-z0-9_-]{1,80}$").matches(id)) return JSONObject().put("error", "bad chat")
        return when (q.optString("op")) {
            "usage" -> usage(m)
            "pending" -> pending(m)
            "home" -> {   // everything the watch home and the chat list need, in one message and fetched side by side
                val u = fan.submit<JSONObject> { usage(m) }; val c = fan.submit<JSONObject> { chats(m) }; val p = fan.submit<JSONObject> { pending(m) }
                JSONObject().apply { for (f in listOf(u, c, p)) { val o = runCatching { f.get() }.getOrElse { JSONObject().put("error", it.message ?: "failed") }; o.keys().forEach { k -> put(k, o.get(k)) } } }
            }
            "decide" -> {
                val kind = q.optString("kind"); val req = q.optString("req"); val v = q.optString("v")
                if (!Regex("^[A-Za-z0-9_-]{1,40}$").matches(req)) return JSONObject().put("error", "bad request")
                val (c, _) = if (kind == "permission") call(m, "POST", "/decision", JSONObject().put("req", req).put("behavior", if (v == "deny") "deny" else "allow")) else call(m, "POST", "/answer", JSONObject().put("req", req).put("answer", v.take(500)))
                JSONObject().put("code", c)
            }
            "theme" -> { val (c, b) = call(m, "GET", "/theme"); if (c != 200) fail(c) else JSONObject(b) }
            "chats" -> chats(m)
            "turn" -> {   // what you asked last, what Claude is doing right now (its last steps) and the newest thing it wrote
                val (c, b) = call(m, "GET", "/sessions/$id/messages?tail=40"); if (c != 200) return fail(c)
                val a = JSONArray(b); var asked = ""; var answer = ""; val steps = ArrayList<String>(); var ts = 0L; var now = ""
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
                JSONObject().put("asked", asked.take(600)).put("answer", answer.take(4000)).put("steps", JSONArray(steps)).put("ts", ts).put("now", now)
                    .put("plan", runCatching { val (pc, pb) = call(m, "GET", "/sessions/$id/plan"); if (pc == 200) JSONObject(pb).optJSONArray("items")?.let { trimPlan(it) } else null }.getOrNull() ?: JSONArray())
            }
            "send" -> JSONObject().put("code", call(m, "POST", "/sessions/$id/send", JSONObject().put("text", q.optString("text").take(4000)).put("mode", "acceptEdits"), lines = 2, read = 20_000).first)
            "stop" -> JSONObject().put("code", call(m, "POST", "/stop", JSONObject().put("session", id)).first)
            else -> JSONObject().put("error", "unknown")
        }
    }

    private fun usage(m: Machine): JSONObject { val (c, b) = call(m, "GET", "/usage"); return if (c != 200) fail(c) else JSONObject().put("usage", JSONObject(b)) }

    private fun pending(m: Machine): JSONObject {
        val (c, b) = call(m, "GET", "/pending"); if (c != 200) return fail(c)
        val src = JSONArray(b); val out = JSONArray()
        for (i in 0 until minOf(src.length(), 6)) { val o = src.getJSONObject(i); out.put(JSONObject().put("req", o.optString("req")).put("kind", o.optString("kind")).put("title", o.optString("title").take(80)).put("text", o.optString("text").take(240)).put("options", o.optJSONArray("options") ?: JSONArray())) }
        return JSONObject().put("items", out)
    }

    /** The chat list for the watch: your pinned chats first (pins and names live on the server, the same as on this phone), then the running ones, then the newest. */
    private fun chats(m: Machine): JSONObject {
        val (c, b) = call(m, "GET", "/sessions"); if (c != 200) return fail(c)
        // pins live on one server for all of them: the same one this phone's chat list uses (the first secure one), so every server's chats get their pins
        val hub = Store.machines(this).filter { !it.guest }.let { l -> l.firstOrNull { it.secure } ?: l.firstOrNull() } ?: m
        val tags = runCatching { val (tc, tb) = call(hub, "GET", "/chat-tags"); if (tc == 200) JSONObject(tb).optJSONObject("tags") else null }.getOrNull() ?: JSONObject()
        val a = JSONArray(b); val out = JSONArray()
        (0 until a.length()).map { a.getJSONObject(it) }.filter { it.optString("kind") != "talk" || it.optString("title").isNotBlank() }
            .sortedWith(compareByDescending<JSONObject> { tags.optJSONObject(it.optString("id"))?.optBoolean("pin") == true }.thenByDescending { it.optBoolean("running") || it.optBoolean("busy") }.thenByDescending { it.optLong("mtime") }).take(20)
            .forEach { val t = tags.optJSONObject(it.optString("id")); out.put(JSONObject().put("id", it.optString("id")).put("title", (t?.optString("name")?.takeIf { n -> n.isNotBlank() } ?: it.optString("title")).take(120)).put("busy", it.optBoolean("running") || it.optBoolean("busy")).put("mtime", it.optLong("mtime")).put("pin", t?.optBoolean("pin") == true)) }
        return JSONObject().put("chats", out)
    }

    /** At most 12 steps, short enough for one data-layer message. */
    private fun trimPlan(a: JSONArray): JSONArray { val out = JSONArray(); for (i in 0 until minOf(a.length(), 12)) { val o = a.getJSONObject(i); out.put(JSONObject().put("text", o.optString("text").take(100)).put("status", o.optString("status"))) }; return out }

    private fun fail(code: Int) = JSONObject().put("code", code).put("error", if (code == 401) "The phone needs to connect again" else "said $code")

    /** One call to the server; for a send only the first lines are read (the run is started, the server keeps going). */
    private fun call(m: Machine, method: String, path: String, body: JSONObject? = null, lines: Int = -1, read: Int = 15_000): Pair<Int, String> {
        val c = (URL(m.base + path).openConnection() as HttpURLConnection).apply {
            connectTimeout = 8_000; readTimeout = read; requestMethod = method
            setRequestProperty("Authorization", "Bearer ${m.token}")
            if (body != null) { doOutput = true; setRequestProperty("Content-Type", "application/json") }
        }
        try {
            if (body != null) c.outputStream.use { it.write(body.toString().toByteArray()) }
            val code = c.responseCode
            if (code != 200) return code to ""
            if (lines >= 0) { c.inputStream.bufferedReader().use { r -> repeat(lines) { r.readLine() ?: return code to "" } }; return code to "" }
            return code to c.inputStream.bufferedReader().use { it.readText() }
        } finally { c.disconnect() }
    }

    override fun onDestroy() { pool.shutdown(); fan.shutdown(); super.onDestroy() }

    companion object { const val PATH = "/hearth/rpc" }
}
