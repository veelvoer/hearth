package dev.voer.hearth

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject

private data class Routine(val id: String, val name: String, val goal: String, val enabled: Boolean, val schedule: String, val mode: String)
private data class Run(val id: String, val name: String, val at: Long, val status: String, val summary: String, val session: String)

private val READY = listOf(
    Triple("Daily digest", "Look at what changed in my projects in the last 24 hours (commits, files, chats) and summarise it in 5 short lines.", JSONObject().put("type", "daily").put("at", "08:00")),
    Triple("Server check", "Check this server: disk space, memory, failed services and the Hearth health. Report only what is wrong, in plain words.", JSONObject().put("type", "daily").put("at", "07:00")),
    Triple("Weekly security look", "Look through my projects for secrets, outdated dependencies and urgent TODOs. Report what you find. Do not change anything.", JSONObject().put("type", "interval").put("minutes", 10080)),
)

private fun whenOf(s: JSONObject?): String = when {
    s == null -> ""
    s.optString("type") == "interval" -> { val m = s.optDouble("minutes", 60.0); if (m >= 1440) "Every ${(m / 1440).toInt()} days" else if (m >= 60) "Every ${(m / 60).toInt()} hours" else "Every ${m.toInt()} minutes" }
    else -> "Every day at " + s.optString("at", "09:00")
}

/** Routines: a goal that Claude works on by itself on a schedule, and a timeline of what it found. */
@Composable
fun RoutinesPage() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    val hub = remember { Store.machines(c).let { l -> l.firstOrNull { it.secure } ?: l.firstOrNull() } }
    var routines by remember { mutableStateOf<List<Routine>?>(null) }
    var runs by remember { mutableStateOf<List<Run>>(emptyList()) }
    var err by remember { mutableStateOf("") }
    var adding by remember { mutableStateOf(false) }
    var name by remember { mutableStateOf("") }
    var goal by remember { mutableStateOf("") }
    var at by remember { mutableStateOf("08:00") }
    var everyHours by remember { mutableStateOf(0) }   // 0: once a day at [at]; otherwise every N hours
    var canEdit by remember { mutableStateOf(false) }
    fun apply(o: JSONObject, t: JSONArray) {
        val a = o.optJSONArray("routines") ?: JSONArray()
        routines = (0 until a.length()).map { a.getJSONObject(it) }.map { Routine(it.optString("id"), it.optString("name"), it.optString("goal"), it.optBoolean("enabled", true), whenOf(it.optJSONObject("schedule")), it.optString("mode")) }
        runs = (0 until t.length()).map { t.getJSONObject(it) }.map { Run(it.optString("id"), it.optString("name"), it.optLong("at"), it.optString("status"), it.optString("summary"), it.optString("session")) }
    }
    fun load(patch: JSONObject? = null) {
        if (hub == null) { err = "Connect a server first."; return }
        scope.launch {
            runCatching { withContext(Dispatchers.IO) {
                val r = if (patch != null) Shares.postJson(hub, "/routines", patch) else JSONObject(Relay.get(hub, "/routines"))
                r to (JSONObject(Relay.get(hub, "/routines/timeline?tail=12")).optJSONArray("runs") ?: JSONArray())
            } }.onSuccess { (r, t) -> apply(r, t); err = "" }
                .onFailure { err = if ((it.message ?: "").let { x -> x.contains("404") || x.contains("unknown", true) || x.contains("not found", true) }) "This server is too old for routines. Update it first." else friendlyError(it, hub.name).substringBefore(" Your message") }
        }
    }
    LaunchedEffect(Unit) { while (true) { load(); delay(10_000) } }
    Txt("Claude works on a goal by itself, on a schedule, and writes down what it found. A new routine only reads, unless you let it edit.", T.small, muted = true, maxLines = 6)
    if (err.isNotBlank()) Txt(err, T.small, color = MaterialTheme.colorScheme.primary)
    val list = routines
    if (list == null && err.isBlank()) { Skeleton(72.dp); Skeleton(72.dp) }
    list?.forEach { r ->
        Card {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) { Txt(r.name, T.body.copy(fontWeight = FontWeight.SemiBold)); Txt(r.schedule + (if (r.mode == "plan") " · reads only" else " · can edit files"), T.small, muted = true) }
                Switch(r.enabled, { load(JSONObject().put("update", JSONObject().put("id", r.id).put("enabled", it))) })
            }
            Txt(r.goal, T.small, muted = true, maxLines = 3)
            Row(verticalAlignment = Alignment.CenterVertically) {
                androidx.compose.runtime.CompositionLocalProvider(LocalIconAlign provides Alignment.CenterStart) { TextAction("Run now") { load(JSONObject().put("run", r.id)) } }
                Spacer(Modifier.weight(1f))
                IconAction("trash", "Delete ${r.name}", Alignment.CenterEnd, 40.dp) { load(JSONObject().put("remove", r.id)) }
            }
        }
    }
    Label("Add a routine")
    ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        READY.forEach { (n, g, s) -> Chip(n, false) { load(JSONObject().put("add", JSONObject().put("name", n).put("goal", g).put("schedule", s).put("mode", "plan"))) } }
        Chip("My own", adding) { adding = !adding }
    }
    if (adding) Card {
        Field(name, { name = it.take(60) }, "Name, e.g. Morning check")
        Spacer(Modifier.height(8.dp))
        Field(goal, { goal = it.take(4000) }, "What should Claude do?", singleLine = false, modifier = Modifier.heightIn(min = 96.dp))
        Spacer(Modifier.height(8.dp))
        Label("When")
        ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf("07:00", "08:00", "12:00", "18:00").forEach { t -> Chip("Daily $t", everyHours == 0 && at == t) { everyHours = 0; at = t } }
            listOf(1, 6, 12).forEach { h -> Chip("Every ${h}h", everyHours == h) { everyHours = h } }
        }
        Spacer(Modifier.height(8.dp))
        ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Chip("Reads only", !canEdit) { canEdit = false }; Chip("Can edit files", canEdit) { canEdit = true } }
        Spacer(Modifier.height(10.dp))
        Button("Add routine", Modifier.fillMaxWidth()) {
            if (name.isBlank() || goal.isBlank()) { err = "Give it a name and a goal."; return@Button }
            val sch = if (everyHours == 0) JSONObject().put("type", "daily").put("at", at) else JSONObject().put("type", "interval").put("minutes", everyHours * 60)
            load(JSONObject().put("add", JSONObject().put("name", name.trim()).put("goal", goal.trim()).put("schedule", sch).put("mode", if (canEdit) "acceptEdits" else "plan")))
            name = ""; goal = ""; adding = false
        }
    }
    if (runs.isNotEmpty()) {
        Label("What it did")
        runs.forEach { r ->
            Card(Modifier.pressable { if (r.session.isNotEmpty() && hub != null) { Ui.pendingOpen = PendingOpen(hub.host, r.session, r.name, "", false); Ui.tab = 0 } }) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.size(10.dp).clip(RoundedCornerShape(50)).background(when (r.status) { "done" -> Color(0xFF4CAF7D); "failed" -> MaterialTheme.colorScheme.primary; else -> MaterialTheme.colorScheme.outlineVariant }))
                    Spacer(Modifier.width(10.dp))
                    Txt(r.name, T.body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.weight(1f))
                    Txt(java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT, java.text.DateFormat.SHORT).format(java.util.Date(r.at)), T.small, muted = true)
                }
                Txt(if (r.status == "running") "Working on it…" else r.summary.replace("**", "").replace("`", "").replace(Regex("(^|\\s)- "), "$1· ").trim().ifBlank { if (r.status == "failed") "It did not finish." else "Done." }, T.small, muted = true, maxLines = 4)
            }
        }
    }
}
