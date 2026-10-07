package dev.voer.hearth

import org.json.JSONArray
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.HttpURLConnection
import java.net.InetAddress
import java.net.SocketTimeoutException
import java.net.URL

data class Machine(val name: String, val host: String, val port: Int, val token: String, val secure: Boolean = false) {
    /** Base URL; a server behind HTTPS is reached on the standard port without showing it. */
    val base get() = (if (secure) "https" else "http") + "://" + host + if ((secure && port == 443) || (!secure && port == 80)) "" else ":$port"
}
data class Sess(val id: String, val title: String, val cwd: String, val mtime: Long, val live: Boolean, val busy: Boolean = false, val running: Boolean = false, val kind: String = "code", val elsewhere: String = "")
data class Msg(val role: String, val text: String, val name: String = "", val detail: String = "", val result: String? = null, val err: Boolean = false)
data class Project(val name: String, val path: String)

/** Turns a pairing link (hearth://pair?url=…&token=…) or a typed address into a computer to connect to. */
fun parseMachine(input: String, name: String, token: String): Machine? {
    val s = input.trim()
    if (s.startsWith("hearth://") || s.startsWith("hearth://")) {
        val u = android.net.Uri.parse(s)
        val url = u.getQueryParameter("url") ?: return null
        val t = u.getQueryParameter("token") ?: return null
        return parseMachine(url, u.getQueryParameter("name") ?: name, t)
    }
    if (s.isEmpty()) return null
    val rest = s.removePrefix("https://").removePrefix("http://").trimEnd('/')
    val hostOnly = rest.substringBefore(':').substringBefore('/')
    // a domain name (not an IP address) with no scheme is a server behind HTTPS
    val secure = s.startsWith("https://") || (!s.startsWith("http://") && hostOnly.contains('.') && hostOnly.any { it.isLetter() })
    val host = rest.substringBefore(':').substringBefore('/')
    val port = rest.substringAfter(':', "").substringBefore('/').toIntOrNull() ?: if (secure) 443 else 47601
    return Machine(name.ifBlank { host }, host, port, token, secure)
}

/** Client for claude_relay.py running on a computer. */
object Relay {
    internal fun open(m: Machine, path: String, post: Boolean = false, read: Int = 10_000): HttpURLConnection =
        (URL("${m.base}$path").openConnection() as HttpURLConnection).apply {
            connectTimeout = 5_000; readTimeout = read
            setRequestProperty("Authorization", "Bearer ${m.token}")
            if (post) { requestMethod = "POST"; doOutput = true; setRequestProperty("Content-Type", "application/json") }
        }

    private fun getJson(m: Machine, path: String): String {
        val c = open(m, path)
        try {
            if (c.responseCode == 401) throw ApiException(401, "Wrong token for ${m.name}")
            if (c.responseCode != 200) throw ApiException(c.responseCode, "Relay error ${c.responseCode}")
            return c.inputStream.bufferedReader().readText()
        } finally { c.disconnect() }
    }

    /** Trades the 6-digit code on the computer's screen for its pairing token. */
    fun pair(m: Machine, code: String): Machine {
        val host = m.host
        val c = open(m, "/pair", post = true)
        try {
            c.outputStream.use { it.write(JSONObject().put("code", code).toString().toByteArray()) }
            val body = (if (c.responseCode == 200) c.inputStream else c.errorStream)?.bufferedReader()?.readText().orEmpty()
            val o = runCatching { JSONObject(body) }.getOrDefault(JSONObject())
            if (c.responseCode != 200) throw ApiException(c.responseCode, o.optString("error").ifBlank { "Pairing failed" }.replaceFirstChar { it.uppercase() })
            return Machine(o.optString("name", host), host, m.port, o.getString("token"), m.secure)
        } finally { c.disconnect() }
    }

    /** Machines on the local network answering the relay's discovery ping (token left empty). */
    fun discover(): List<Machine> {
        val found = LinkedHashMap<String, Machine>()
        runCatching {
            DatagramSocket().use { s ->
                s.broadcast = true; s.soTimeout = 1200
                val ping = "CLAUDE_METER_DISCOVER".toByteArray()
                s.send(DatagramPacket(ping, ping.size, InetAddress.getByName("255.255.255.255"), 47600))
                val buf = ByteArray(512)
                while (true) {
                    val p = DatagramPacket(buf, buf.size)
                    try { s.receive(p) } catch (_: SocketTimeoutException) { break }
                    val o = JSONObject(String(p.data, 0, p.length))
                    val host = p.address.hostAddress ?: continue
                    found[host] = Machine(o.optString("name", host), host, o.optInt("port", 47601), "")
                }
            }
        }
        return found.values.toList()
    }

    fun sessions(m: Machine): List<Sess> {
        val a = JSONArray(getJson(m, "/sessions"))
        return List(a.length()) {
            val o = a.getJSONObject(it)
            Sess(o.getString("id"), o.getString("title"), o.optString("cwd"), o.getLong("mtime"), o.optBoolean("live"), o.optBoolean("busy"), o.optBoolean("running"), o.optString("kind", "code").ifBlank { "code" }, o.optString("elsewhere", ""))
        }
    }

    fun messages(m: Machine, id: String): List<Msg> {
        val a = JSONArray(getJson(m, "/sessions/$id/messages"))
        return List(a.length()) {
            val o = a.getJSONObject(it)
            val inp = o.optJSONObject("input")
            val detail = inp?.let { i -> listOf("command", "file_path", "pattern", "path", "url").firstNotNullOfOrNull { k -> i.optString(k).takeIf { v -> v.isNotBlank() } } }.orEmpty()
            Msg(o.getString("role"), o.getString("text"), o.optString("name"), detail, if (o.has("result")) o.optString("result") else null, o.optBoolean("error"))
        }
    }

    fun projects(m: Machine): Pair<String?, List<Project>> {
        val o = JSONObject(getJson(m, "/projects"))
        val a = o.getJSONArray("projects")
        return (if (o.isNull("root")) null else o.optString("root")) to List(a.length()) { a.getJSONObject(it).let { p -> Project(p.getString("name"), p.getString("path")) } }
    }

    fun createProject(m: Machine, name: String): String {
        val c = open(m, "/projects", post = true)
        try {
            c.outputStream.use { it.write(JSONObject().put("name", name).toString().toByteArray()) }
            val body = (if (c.responseCode == 200) c.inputStream else c.errorStream)?.bufferedReader()?.readText().orEmpty()
            val o = runCatching { JSONObject(body) }.getOrDefault(JSONObject())
            if (c.responseCode != 200) throw ApiException(c.responseCode, o.optString("error").ifBlank { "Couldn't create the project" })
            return o.getString("path")
        } finally { c.disconnect() }
    }

    /** Plan limits as reported by the real Claude Code on that computer (no account tokens are involved). Null until it has run once. */
    fun usage(m: Machine): Usage? {
        val o = JSONObject(getJson(m, "/usage"))
        if (!o.has("at")) return null
        fun lim(k: String) = o.optJSONObject(k)?.let { Limit(it.getDouble("pct"), if (it.isNull("resetsAt")) null else it.getLong("resetsAt")) }
        return Usage(lim("session"), lim("week"), lim("opus"), lim("sonnet"), null, o.getLong("at"))
    }

    /** Asks that computer to read the limits again with one tiny request. Only when you press Refresh. */
    fun refreshUsage(m: Machine) {
        val c = open(m, "/usage/refresh", post = true, read = 90_000)
        try { c.outputStream.use { it.write("{}".toByteArray()) }; c.responseCode } finally { c.disconnect() }
    }

    /** Questions and permission requests waiting for you on [m]. */
    fun pending(m: Machine): List<CallEvent> {
        val a = JSONArray(getJson(m, "/pending"))
        return List(a.length()) { val o = a.getJSONObject(it)
            CallEvent(o.optLong("id"), o.getString("kind"), o.optString("session"), o.optString("title"), o.optString("cwd"), o.optString("text"), 0, o.optString("req"), o.optString("machine"), o.optBoolean("call", true), "", o.optString("question"),
                o.optJSONArray("options")?.let { op -> List(op.length()) { i -> op.getJSONObject(i).optString("label") } } ?: emptyList(), o.optBoolean("multi")) }
    }

    fun answer(m: Machine, req: String, text: String) {
        val c = open(m, "/answer", post = true)
        try { c.outputStream.use { it.write(JSONObject().put("req", req).put("answer", text).toString().toByteArray()) }; c.responseCode } finally { c.disconnect() }
    }

    /** "off", "done" (call me once when it's done) or "attention" (call me when it's done or needs me). */
    fun prefs(m: Machine, id: String): String = JSONObject(getJson(m, "/sessions/$id/prefs")).optString("call", "off")

    /** Where this chat runs: "auto" (your laptop when it is online, else the server), "laptop" (wait for it) or "vps". */
    fun runPref(m: Machine, id: String): String = JSONObject(getJson(m, "/sessions/$id/prefs")).optString("run", "auto")

    fun setRun(m: Machine, id: String, run: String) {
        val c = open(m, "/sessions/$id/prefs", post = true)
        try { c.outputStream.use { it.write(JSONObject().put("run", run).toString().toByteArray()) }; c.responseCode } finally { c.disconnect() }
    }

    fun setPrefs(m: Machine, id: String, call: String) {
        val c = open(m, "/sessions/$id/prefs", post = true)
        try { c.outputStream.use { it.write(JSONObject().put("call", call).toString().toByteArray()) }; c.responseCode } finally { c.disconnect() }
    }

    /** Saves a photo or file inside the project on [m] and returns its path there, so a message can point at it. */
    fun upload(m: Machine, sessionId: String, cwd: String, name: String, bytes: ByteArray): String {
        val q = if (sessionId.isNotEmpty()) "session=" + java.net.URLEncoder.encode(sessionId, "UTF-8") else "cwd=" + java.net.URLEncoder.encode(cwd, "UTF-8")
        val c = open(m, "/upload?$q&name=" + java.net.URLEncoder.encode(name, "UTF-8"), post = true, read = 120_000)
        try {
            c.setRequestProperty("Content-Type", "application/octet-stream")
            c.setFixedLengthStreamingMode(bytes.size)
            c.outputStream.use { it.write(bytes) }
            val body = (if (c.responseCode == 200) c.inputStream else c.errorStream)?.bufferedReader()?.readText().orEmpty()
            val o = runCatching { JSONObject(body) }.getOrDefault(JSONObject())
            if (c.responseCode != 200) throw ApiException(c.responseCode, o.optString("error").ifBlank { "Upload failed" }.replaceFirstChar { it.uppercase() })
            return o.getString("path")
        } finally { c.disconnect() }
    }

    fun stop(m: Machine, id: String) {
        val c = open(m, "/stop", post = true)
        try { c.outputStream.use { it.write(JSONObject().put("session", id).toString().toByteArray()) }; c.responseCode } finally { c.disconnect() }
    }

    /** Streams one turn. [onEvent] gets ("delta"|"tool"|"done", text, isError). Blocks until the turn ends. */
    fun send(m: Machine, id: String, text: String, mode: String, brief: Boolean = false, cwd: String? = null, kind: String? = null, model: String? = null, effort: String? = null, onEvent: (String, String, Boolean) -> Unit) {
        val c = open(m, "/sessions/${id.ifEmpty { "new" }}/send", post = true, read = 60 * 60_000)
        try {
            val body = JSONObject().put("text", text).put("mode", mode).put("brief", brief)
            if (id.isEmpty() && cwd != null) body.put("cwd", cwd)
            if (id.isEmpty() && kind != null) body.put("kind", kind)
            if (model != null) body.put("model", model)
            if (effort != null) body.put("effort", effort)
            c.outputStream.use { it.write(body.toString().toByteArray()) }
            if (c.responseCode != 200) {
                val msg = runCatching { JSONObject(c.errorStream?.bufferedReader()?.readText().orEmpty()).optString("error") }.getOrNull()
                throw ApiException(c.responseCode, msg?.takeIf { it.isNotBlank() }?.replaceFirstChar { it.uppercase() } ?: "Relay error ${c.responseCode}")
            }
            c.inputStream.bufferedReader().forEachLine { line ->
                val o = runCatching { JSONObject(line) }.getOrNull() ?: return@forEachLine
                val t = o.optString("t")
                val payload = when (t) { "session" -> o.optString("id"); "prefs" -> o.optString("call"); "queued" -> if (o.optBoolean("online")) "now" else "later"; "tool" -> o.optString("name") + "\u0000" + (o.optJSONObject("input")?.let { i -> listOf("command", "file_path", "pattern", "path", "url").firstNotNullOfOrNull { k -> i.optString(k).takeIf { v -> v.isNotBlank() } } }.orEmpty()) + "\u0000" + o.optString("id")
                    "result" -> o.optString("id") + "\u0000" + o.optString("text"); else -> o.optString("text") }
                onEvent(t, payload, o.optBoolean("error"))
            }
        } finally { c.disconnect() }
    }
}
