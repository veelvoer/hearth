package dev.voer.hearth

import org.json.JSONArray
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.HttpURLConnection
import java.net.InetAddress
import java.net.SocketTimeoutException
import java.net.URL

data class Machine(val name: String, val host: String, val port: Int, val token: String, val secure: Boolean = false, val guest: Boolean = false, val label: String = "", val share: String = "") {
    /** Base URL; a server behind HTTPS is reached on the standard port without showing it. */
    val base get() = (if (secure) "https" else "http") + "://" + host + if ((secure && port == 443) || (!secure && port == 80)) "" else ":$port"
}
data class Sess(val id: String, val title: String, val cwd: String, val mtime: Long, val live: Boolean, val busy: Boolean = false, val running: Boolean = false, val kind: String = "code", val elsewhere: String = "", val tokens: Long = 0, val model: String = "", val root: Boolean = false, val shared: Int = -1)
data class Msg(val role: String, val text: String, val name: String = "", val detail: String = "", val result: String? = null, val err: Boolean = false, val own: Boolean = false, val ts: Long = 0)
data class Project(val name: String, val path: String)
data class Computer(val id: String, val name: String, val online: Boolean, val kind: String)

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

    /** Asks a computer on this Wi-Fi to let this phone in. The person at the computer presses Accept; no address or code is typed. Returns the request id. */
    fun askToConnect(m: Machine, device: String): String {
        val c = open(m, "/pair/request", post = true)
        try {
            c.outputStream.use { it.write(JSONObject().put("name", device).toString().toByteArray()) }
            val code = c.responseCode
            val body = (if (code == 200) c.inputStream else c.errorStream)?.bufferedReader()?.readText() ?: ""
            if (code != 200) throw ApiException(code, runCatching { JSONObject(body).optString("error") }.getOrNull()?.ifBlank { null } ?: "The computer said $code")
            return JSONObject(body).getString("id")
        } finally { c.disconnect() }
    }
    fun askStatus(m: Machine, id: String): JSONObject {
        val c = open(m, "/pair/request/$id")
        try { return JSONObject(c.inputStream.bufferedReader().readText()) } finally { c.disconnect() }
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
            Sess(o.getString("id"), o.getString("title"), o.optString("cwd"), o.getLong("mtime"), o.optBoolean("live"), o.optBoolean("busy"), o.optBoolean("running"), o.optString("kind", "code").ifBlank { "code" }, o.optString("elsewhere", ""), o.optLong("tokens", 0), o.optString("model", ""), o.optBoolean("root"), o.optInt("shared", -1))
        }
    }

    fun messages(m: Machine, id: String): List<Msg> {
        val a = JSONArray(getJson(m, "/sessions/$id/messages"))
        return List(a.length()) {
            val o = a.getJSONObject(it)
            val inp = o.optJSONObject("input")
            val detail = inp?.let { i -> listOf("command", "file_path", "pattern", "path", "url").firstNotNullOfOrNull { k -> i.optString(k).takeIf { v -> v.isNotBlank() } } }.orEmpty()
            Msg(o.getString("role"), o.getString("text"), o.optString("name"), detail, if (o.has("result")) o.optString("result") else null, o.optBoolean("error"), o.optBoolean("own"), o.optLong("ts"))
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

    private fun parsePlan(o: JSONObject): WeekPlan {
        val a = o.optJSONArray("days")
        val days = if (a == null) emptyList() else List(a.length()) { a.getJSONObject(it).let { d -> PlanDay(d.getInt("wd"), d.getString("name"), d.getString("state"), d.getDouble("used"), d.getDouble("planned"), d.optBoolean("fixed"), if (d.isNull("target")) null else d.getDouble("target")) } }
        val sa = o.optJSONArray("segs")
        val segs = if (sa == null) emptyList() else List(sa.length()) { sa.getJSONObject(it).let { d -> PlanSeg(d.getInt("wd"), d.getDouble("f"), d.getString("state"), d.getDouble("used"), d.getDouble("w")) } }
        val to = o.optJSONObject("targets")
        val targets = HashMap<Int, Double>().also { m -> to?.keys()?.forEach { k -> k.toIntOrNull()?.let { wd -> m[wd] = to.getDouble(k) } } }
        return WeekPlan(if (o.has("unavailable")) o.getString("unavailable") else null, o.optDouble("left", 0.0), o.optBoolean("over"), o.optDouble("leftover", 0.0), o.optDouble("unknown", 0.0), o.optInt("autoDays"), o.optDouble("autoEach", 0.0), days, segs, targets, o.optDouble("R", 0.0))
    }

    /** The week planned day by day (the computer works it out, so the phone and the desktop always agree). [tz] = minutes east of UTC. */
    fun weekPlan(m: Machine, tz: Int): WeekPlan = parsePlan(JSONObject(getJson(m, "/weekplan?tz=$tz")))

    /** Pins [day] (0 = Sunday) to [pct] percent of the week, or gives it back to even sharing when [pct] is null. */
    fun setWeekDay(m: Machine, day: Int, pct: Double?, tz: Int): WeekPlan {
        val c = open(m, "/weekplan", post = true)
        try {
            c.outputStream.use { it.write(JSONObject().put("day", day).put("pct", pct ?: JSONObject.NULL).put("tz", tz).toString().toByteArray()) }
            val body = (if (c.responseCode == 200) c.inputStream else c.errorStream)?.bufferedReader()?.readText().orEmpty()
            val o = runCatching { JSONObject(body) }.getOrDefault(JSONObject())
            if (c.responseCode != 200) throw ApiException(c.responseCode, o.optString("error").ifBlank { "Couldn't save" })
            return parsePlan(o)
        } finally { c.disconnect() }
    }

    /** What the server keeps in sync: chats and project files, and which of your computers are linked. */
    fun syncText(m: Machine): String {
        val o = JSONObject(getJson(m, "/sync/status"))
        val parts = ArrayList<String>()
        if (o.has("chats")) parts += o.optInt("chats").let { n -> "$n chat" + if (n == 1) "" else "s" }
        o.optJSONObject("files")?.let { f -> if (f.has("count")) parts += f.optInt("count").let { n -> "$n project file" + if (n == 1) "" else "s" } }
        val pcs = o.optJSONArray("computers")
        if (pcs != null && pcs.length() > 0) parts += (0 until pcs.length()).joinToString(", ") { i -> pcs.getJSONObject(i).let { c -> c.optString("name") + if (c.optBoolean("online")) " online" else " offline" } }
        return if (parts.isEmpty()) "Nothing is synced here yet" else parts.joinToString(" · ")
    }

    data class MemInfo(val pct: Int, val availableMb: Int, val totalMb: Int, val usedMb: Int, val canRestart: Boolean)
    fun memory(m: Machine): MemInfo = JSONObject(getJson(m, "/admin/memory")).let { MemInfo(it.getInt("pct"), it.getInt("availableMb"), it.getInt("totalMb"), it.getInt("usedMb"), it.optBoolean("canRestart")) }
    fun setMemory(m: Machine, pct: Int, restart: Boolean) {
        val c = open(m, "/admin/memory", post = true)
        try {
            c.outputStream.use { it.write(JSONObject().put("pct", pct).put("restart", restart).toString().toByteArray()) }
            val body = (if (c.responseCode == 200) c.inputStream else c.errorStream)?.bufferedReader()?.readText().orEmpty()
            if (c.responseCode != 200) throw ApiException(c.responseCode, runCatching { JSONObject(body).optString("error") }.getOrDefault("").ifBlank { "Couldn't save" })
        } finally { c.disconnect() }
    }

    /** Plan limits as reported by the real Claude Code on that computer (no account tokens are involved). Null until it has run once. */
    fun usage(m: Machine): Usage? {
        val o = JSONObject(getJson(m, "/usage"))
        if (!o.has("at")) return null
        fun lim(k: String) = o.optJSONObject(k)?.let { Limit(it.getDouble("pct"), if (it.isNull("resetsAt")) null else it.getLong("resetsAt")) }
        return Usage(lim("session"), lim("week"), lim("opus"), lim("sonnet"), null, o.getLong("at"))
    }

    /** What the last 30 days would have cost at API prices, and the cost of each of the last [days] days (the Home screen's tile and chart). */
    fun cost(m: Machine, days: Int = 14): ApiCost? {
        val o = JSONObject(getJson(m, "/usage-report?days=30"))
        val a = o.optJSONArray("days") ?: return null
        val all = List(a.length()) { a.getJSONObject(it).optDouble("cost", 0.0) }
        return ApiCost(o.optJSONObject("totals")?.optDouble("cost", 0.0) ?: all.sum(), all.takeLast(days))
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
                o.optJSONArray("options")?.let { op -> List(op.length()) { i -> op.getJSONObject(i).optString("label") } } ?: emptyList(), o.optBoolean("multi"), o.optString("tool"), previewOf(o.optString("tool"), o.optJSONObject("input")), o.optJSONObject("suggest")?.optString("label").orEmpty()) }
    }

    /** What the server allows (work in any folder, Full auto, root chats) and the commands allowed for good. */
    data class Rule(val id: String, val label: String)
    data class Options(val outside: Boolean, val outsideAvailable: Boolean, val bypass: Boolean, val bypassFixed: Boolean, val root: Boolean, val rootOk: Boolean, val rootWhy: String, val rules: List<Rule>)
    private fun parseOptions(o: JSONObject): Options {
        val ra = o.optJSONArray("rules"); val rr = o.optJSONObject("rootReady")
        return Options(o.optBoolean("outside", true), o.optBoolean("outsideAvailable"), o.optBoolean("bypass"), o.optBoolean("bypassFixed"), o.optBoolean("root"), rr?.optBoolean("ok") ?: false, rr?.optString("why").orEmpty(),
            if (ra == null) emptyList() else List(ra.length()) { ra.getJSONObject(it).let { r -> Rule(r.getString("id"), r.optString("label").ifBlank { r.optString("prefix") }) } })
    }
    data class Skill(val name: String, val desc: String, val src: String, val own: Boolean)
    /** Every skill and slash command this computer knows: yours, from your Claude account, plugins. */
    fun skills(m: Machine): List<Skill> {
        val a = org.json.JSONArray(getJson(m, "/commands"))
        return (0 until a.length()).map { a.getJSONObject(it) }.filter { it.optString("src") != "built in" }.map { Skill(it.getString("name"), it.optString("desc"), it.optString("src"), it.optBoolean("own")) }
    }
    fun options(m: Machine): Options = parseOptions(JSONObject(getJson(m, "/admin/options")))
    private fun postJson(m: Machine, path: String, body: JSONObject): JSONObject {
        val c = open(m, path, post = true)
        try {
            c.outputStream.use { it.write(body.toString().toByteArray()) }
            val text = (if (c.responseCode == 200) c.inputStream else c.errorStream)?.bufferedReader()?.readText().orEmpty()
            val o = runCatching { JSONObject(text) }.getOrDefault(JSONObject())
            if (c.responseCode != 200) throw ApiException(c.responseCode, o.optString("error").ifBlank { "Couldn't save" })
            return o
        } finally { c.disconnect() }
    }
    fun setOptions(m: Machine, body: JSONObject): Options = parseOptions(postJson(m, "/admin/options", body))
    fun setAuto(m: Machine, id: String, on: Boolean) { postJson(m, "/sessions/$id/auto", JSONObject().put("on", on)) }

    data class Dir(val name: String, val path: String, val git: Boolean)
    data class Listing(val path: String, val parent: String?, val dirs: List<Dir>)
    fun browse(m: Machine, path: String, hidden: Boolean): Listing {
        val o = JSONObject(getJson(m, "/fs?path=" + java.net.URLEncoder.encode(path, "UTF-8") + (if (hidden) "&hidden=1" else "")))
        val a = o.getJSONArray("dirs")
        return Listing(o.getString("path"), if (o.isNull("parent")) null else o.optString("parent"), List(a.length()) { a.getJSONObject(it).let { d -> Dir(d.getString("name"), d.getString("path"), d.optBoolean("git")) } })
    }
    fun makeDir(m: Machine, parent: String, name: String): String = postJson(m, "/fs/mkdir", JSONObject().put("path", parent).put("name", name)).getString("path")

    /** What a tool wants to do, as lines for the approval prompt: the command, or the lines that change (- old, + new). */
    private fun previewOf(tool: String, i: JSONObject?): String {
        if (i == null) return ""
        fun lines(prefix: String, t: String) = t.split("\n").joinToString("\n") { prefix + it }
        return when (tool) {
            "Bash" -> "$ " + i.optString("command")
            "Edit", "NotebookEdit" -> i.optString("file_path") + "\n" + lines("- ", i.optString("old_string")) + "\n" + lines("+ ", i.optString("new_string"))
            "Write" -> i.optString("file_path") + "\n" + lines("+ ", i.optString("content").lines().take(10).joinToString("\n"))
            else -> ""
        }
    }

    fun answer(m: Machine, req: String, text: String) {
        val c = open(m, "/answer", post = true)
        try { c.outputStream.use { it.write(JSONObject().put("req", req).put("answer", text).toString().toByteArray()) }; c.responseCode } finally { c.disconnect() }
    }

    /** "off", "done" (call me once when it's done) or "attention" (call me when it's done or needs me). */
    /** The computers of the user that are linked to this server (and the server itself). */
    fun computers(m: Machine): List<Computer> { val a = JSONArray(getJson(m, "/computers")); return List(a.length()) { val o = a.getJSONObject(it); Computer(o.getString("id"), o.getString("name"), o.optBoolean("online"), o.optString("kind")) } }
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
    fun send(m: Machine, id: String, text: String, mode: String, brief: Boolean = false, cwd: String? = null, kind: String? = null, model: String? = null, effort: String? = null, run: String? = null, root: Boolean = false, onEvent: (String, String, Boolean) -> Unit) {
        var lastN = -1; var finished = false; var chat = id
        /** One line of the run. Events carry a running number, so one that was already seen (after joining the run again) is skipped. */
        fun take(line: String) {
            val o = runCatching { JSONObject(line) }.getOrNull() ?: return
            val n = o.optInt("n", -1)
            if (n >= 0) { if (n <= lastN) return; lastN = n }
            val t = o.optString("t")
            if (t == "session" && o.optString("id").isNotEmpty()) chat = o.optString("id")
            if (t == "end" || t == "fail") finished = true
            val payload = when (t) { "session" -> o.optString("id"); "prefs" -> o.optString("call"); "queued" -> if (o.optBoolean("approval")) "approval:" + o.optString("to") else if (o.optBoolean("auto")) "auto:" + o.optString("to") else if (o.optBoolean("online")) "now" else "later"; "tool" -> o.optString("name") + "\u0000" + (o.optJSONObject("input")?.let { i -> listOf("command", "file_path", "pattern", "path", "url").firstNotNullOfOrNull { k -> i.optString(k).takeIf { v -> v.isNotBlank() } } }.orEmpty()) + "\u0000" + o.optString("id")
                "result" -> o.optString("id") + "\u0000" + o.optString("text"); else -> o.optString("text") }
            onEvent(t, payload, o.optBoolean("error"))
        }
        val c = open(m, "/sessions/${id.ifEmpty { "new" }}/send", post = true, read = 5 * 60_000)
        try {
            val body = JSONObject().put("text", text).put("mode", mode).put("brief", brief)
            if (id.isEmpty() && cwd != null) body.put("cwd", cwd)
            if (id.isEmpty() && kind != null) body.put("kind", kind)
            if (model != null) body.put("model", model)
            if (effort != null) body.put("effort", effort)
            if (run != null) body.put("run", run)
            if (id.isEmpty() && root) body.put("root", true)
            c.outputStream.use { it.write(body.toString().toByteArray()) }
            if (c.responseCode != 200) {
                val msg = runCatching { JSONObject(c.errorStream?.bufferedReader()?.readText().orEmpty()).optString("error") }.getOrNull()
                throw ApiException(c.responseCode, msg?.takeIf { it.isNotBlank() }?.replaceFirstChar { it.uppercase() } ?: "Relay error ${c.responseCode}")
            }
            c.inputStream.bufferedReader().forEachLine { take(it) }
        } catch (e: ApiException) { throw e } catch (e: java.io.IOException) { /* the connection dropped: join the run again below */ } finally { c.disconnect() }
        // The connection dropped (mobile network, sleep, a proxy) before the run said it was over. The run goes on at the computer, so join it again where we left off.
        var attempt = 0
        while (!finished) {
            attempt++
            if (attempt > 60 || chat.isEmpty()) throw ApiException(0, "Lost the connection to ${m.name}. Claude may still be working; open the chat again in a moment.")
            Thread.sleep(minOf(4000L, 400L * attempt))
            val r = open(m, "/sessions/$chat/stream?from=${lastN + 1}", read = 5 * 60_000)
            try {
                when (r.responseCode) {
                    404 -> { onEvent("end", "", false); finished = true }   // no run any more: it finished while we were away
                    200 -> { attempt = 0; r.inputStream.bufferedReader().forEachLine { take(it) } }
                }
            } catch (e: java.io.IOException) { /* still unreachable: try again */ } finally { r.disconnect() }
        }
    }

    /** Is Claude working on this chat right now? Null when this server is too old to say. */
    fun working(m: Machine, id: String): Boolean? = runCatching {
        val o = JSONObject(getJson(m, "/sessions/$id/state")); o.optBoolean("busy")
    }.getOrNull()
}
