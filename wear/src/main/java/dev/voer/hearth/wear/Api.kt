package dev.voer.hearth.wear

import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import org.json.JSONArray
import org.json.JSONObject

data class Usage(val session: Double?, val week: Double?, val sessionReset: Long?)
data class Chat(val id: String, val title: String, val busy: Boolean, val mtime: Long, val server: Server)
data class Turn(val asked: String, val answer: String)

/** The few calls the watch needs, over the same HTTPS link as the phone (the watch borrows the phone's connection when it has none). */
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
        val o = JSONObject(get(s, "/usage"))
        fun pct(k: String) = o.optJSONObject(k)?.optDouble("pct")?.takeIf { !it.isNaN() }
        return Usage(pct("session"), pct("week"), o.optJSONObject("session")?.let { if (it.isNull("resetsAt")) null else it.optLong("resetsAt") })
    }

    fun chats(s: Server): List<Chat> {
        val a = JSONArray(get(s, "/sessions"))
        return (0 until a.length()).map { a.getJSONObject(it) }.filter { it.optString("kind") != "talk" || it.optString("title").isNotBlank() }
            .map { Chat(it.getString("id"), it.optString("title").ifBlank { "Chat" }, it.optBoolean("running") || it.optBoolean("busy"), it.optLong("mtime"), s) }
    }

    /** The last thing you asked and the last answer, without tool calls. */
    fun lastTurn(s: Server, id: String): Turn {
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
        val code = post(s, "/sessions/$id/send", JSONObject().put("text", text).put("mode", "acceptEdits"), read = 20_000)
        if (code == 409) throw IOException("Claude is still busy with this chat")
        if (code != 200) throw IOException("${s.name} said $code")
    }

    fun stop(s: Server, id: String) { post(s, "/stop", JSONObject().put("session", id)) }
}

/** Claude's markdown, made readable on a small round screen. */
fun plain(md: String): String = md.replace("**", "").replace("`", "").lines().joinToString("\n") { l ->
    val t = l.trimStart(); when { t.startsWith("#") -> t.trimStart('#').trim(); t.startsWith("- ") || t.startsWith("* ") -> "• " + t.drop(2); else -> l }
}.replace(Regex("\n{3,}"), "\n\n").trim()
