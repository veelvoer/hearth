package dev.voer.hearth

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

private fun hubOf(c: android.content.Context): Machine? = Store.machines(c).let { l -> l.firstOrNull { it.secure } ?: l.firstOrNull() }

/** What keeps you safe while Claude works: a snapshot before every run, a spending limit, and nightly backups of the chats. */
@Composable
fun SafetyCard() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var data by remember { mutableStateOf<JSONObject?>(null) }
    var err by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    val hub = remember { hubOf(c) }
    fun call(patch: JSONObject?) {
        if (hub == null) { err = "Connect a server first."; return }
        busy = true
        scope.launch {
            runCatching { withContext(Dispatchers.IO) { if (patch == null) Relay.safety(hub) else Relay.setSafety(hub, patch) } }
                .onSuccess { data = it; err = ""; if (it.optBoolean("ok") && patch?.has("backupNow") == true) Toast.makeText(c, "Backup made", Toast.LENGTH_SHORT).show() }
                .onFailure { err = if ((it.message ?: "").let { x -> x.contains("404") || x.contains("unknown", true) || x.contains("not found", true) }) "This server is too old for this. Update it first." else friendlyError(it, hub.name).substringBefore(" Your message") }
            busy = false
        }
    }
    LaunchedEffect(Unit) { call(null) }
    Card {
        Head("shield", "Safety", "Before every run Hearth keeps a copy of the project, so you can undo everything Claude did. A budget stops runs that would cost more than you want. Backups copy your chats every night.")
        if (err.isNotBlank()) { Spacer(Modifier.height(8.dp)); Txt(err, T.small, color = MaterialTheme.colorScheme.primary) }
        val d = data
        if (d == null) { Spacer(Modifier.height(10.dp)); Skeleton(56.dp, radius = 14.dp); return@Card }
        Row2("Plain language", "Claude explains in everyday words, without jargon or code unless you ask.") { Switch(d.optBoolean("plain", false), { call(JSONObject().put("plain", it)) }) }
        Row2("Snapshot before every run", "A copy of the project folder, kept for the last 8 runs. In a chat: Options, Undo this run.") { Switch(d.optBoolean("snapshots", true), { call(JSONObject().put("snapshots", it)) }) }
        val b = d.optJSONObject("budget") ?: JSONObject()
        Spacer(Modifier.height(6.dp))
        Label("Spending limit per day · used ${"$%.2f".format(b.optDouble("usedDay", 0.0))}")
        ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(0, 5, 10, 25, 50, 100).forEach { v -> Chip(if (v == 0) "Off" else "$$v", b.optInt("perDay") == v) { call(JSONObject().put("budget", JSONObject().put("perDay", v).put("perChat", b.optInt("perChat")))) } }
        }
        Spacer(Modifier.height(6.dp))
        Label("Limit per chat")
        ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(0, 2, 5, 10, 25).forEach { v -> Chip(if (v == 0) "Off" else "$$v", b.optInt("perChat") == v) { call(JSONObject().put("budget", JSONObject().put("perDay", b.optInt("perDay")).put("perChat", v))) } }
        }
        val bk = d.optJSONObject("backups") ?: JSONObject()
        Spacer(Modifier.height(6.dp))
        Row2("Nightly backup of the chats", "Between 2 and 5 at night, the last 7 are kept. Restoring never overwrites a chat that is newer.") { Switch(bk.optBoolean("on"), { call(JSONObject().put("backups", it)) }) }
        val list = bk.optJSONArray("list")
        val last = list?.optJSONObject(0)
        Row(verticalAlignment = Alignment.CenterVertically) {
            Txt(if (last != null) "Last: " + java.text.DateFormat.getDateTimeInstance(java.text.DateFormat.SHORT, java.text.DateFormat.SHORT).format(java.util.Date(last.optLong("at"))) else "No backup yet", T.small, muted = true, modifier = Modifier.weight(1f))
            TextAction(if (busy) "Working…" else "Back up now") { if (!busy) call(JSONObject().put("backupNow", true)) }
        }
        if (last != null) androidx.compose.runtime.CompositionLocalProvider(LocalIconAlign provides Alignment.CenterStart) { TextAction("Restore the last backup") { call(JSONObject().put("restoreBackup", last.optString("name"))) } }
    }
}

/** Rules that always say no, and what was decided lately. */
@Composable
fun RulesCard() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    val hub = remember { hubOf(c) }
    var deny by remember { mutableStateOf<List<Pair<String, String>>?>(null) }
    var log by remember { mutableStateOf<List<Triple<Long, String, String>>>(emptyList()) }
    var word by remember { mutableStateOf("") }
    var err by remember { mutableStateOf("") }
    fun load() { if (hub == null) return; scope.launch { runCatching { withContext(Dispatchers.IO) { Relay.rules(hub) to Relay.decisions(hub) } }.onSuccess { (o, l) -> val a = o.optJSONArray("denyRules"); deny = (0 until (a?.length() ?: 0)).map { a!!.getJSONObject(it).let { x -> x.optString("id") to x.optString("label") } }; log = l; err = "" }.onFailure { err = if ((it.message ?: "").contains("404")) "This server is too old for this. Update it first." else friendlyError(it, hub.name).substringBefore(" Your message") } } }
    LaunchedEffect(Unit) { load() }
    Card {
        Head("lock", "Never allow", "Claude asks before it runs a command. A word you add here is refused at once, without asking: for example \"rm -rf\" or \"sudo\". Below you see what was allowed and refused lately.")
        if (err.isNotBlank()) { Spacer(Modifier.height(8.dp)); Txt(err, T.small, color = MaterialTheme.colorScheme.primary) }
        deny?.forEach { (id, label) ->
            Row(verticalAlignment = Alignment.CenterVertically) {
                Txt(label, T.body.copy(fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace), modifier = Modifier.weight(1f))
                IconAction("trash", "Remove $label", Alignment.CenterEnd, 40.dp) { if (hub != null) scope.launch { runCatching { withContext(Dispatchers.IO) { Relay.ruleChange(hub, JSONObject().put("removeDeny", id)) } }; load() } }
            }
        }
        Spacer(Modifier.height(6.dp))
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.weight(1f)) { Field(word, { word = it }, "Command words, e.g. rm -rf") }
            IconAction("plus", "Add rule", Alignment.CenterEnd, 48.dp) { if (hub != null && word.isNotBlank()) scope.launch { runCatching { withContext(Dispatchers.IO) { Relay.ruleChange(hub, JSONObject().put("addDeny", word.trim())) } }; word = ""; load() } }
        }
        if (log.isNotEmpty()) {
            Spacer(Modifier.height(10.dp)); Label("Lately")
            log.forEach { (at, result, text) ->
                Spacer(Modifier.height(6.dp))
                Row(verticalAlignment = Alignment.Top) {
                    val good = result.startsWith("allowed"); val bad = result.startsWith("denied")
                    Box(Modifier.padding(top = 6.dp).size(8.dp).clip(RoundedCornerShape(50)).background(if (bad) MaterialTheme.colorScheme.primary else if (good) androidx.compose.ui.graphics.Color(0xFF4CAF7D) else MaterialTheme.colorScheme.outlineVariant))
                    Spacer(Modifier.width(10.dp))
                    Column(Modifier.weight(1f)) { Txt(text, T.small.copy(fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace, fontSize = 12.sp), maxLines = 1); Txt(result, T.small.copy(fontSize = 12.sp), muted = true) }
                }
            }
        }
    }
}

/** Last week at a glance, once a week on Home. Gone for the week after you close it. */
@Composable
fun RecapCard() {
    val c = LocalContext.current
    val hub = remember { hubOf(c) } ?: return
    val week = remember { java.time.LocalDate.now().let { it.year * 100 + it.get(java.time.temporal.IsoFields.WEEK_OF_WEEK_BASED_YEAR) } }
    var shown by remember { mutableStateOf(Store.recapClosed(c) != week) }
    var d by remember { mutableStateOf<JSONObject?>(null) }
    LaunchedEffect(Unit) { if (shown) d = runCatching { withContext(Dispatchers.IO) { Relay.recap(hub) } }.getOrNull() }
    val r = d ?: return
    if (!shown || r.optBoolean("empty", true) || java.time.LocalDate.now().dayOfWeek.value > 2) return   // Monday and Tuesday only
    Card {
        Row(verticalAlignment = Alignment.CenterVertically) { Label("Your last 7 days", Modifier.weight(1f)); IconAction("x", "Close", Alignment.CenterEnd, 40.dp) { Store.setRecapClosed(c, week); shown = false } }
        Spacer(Modifier.height(6.dp))
        Txt("You used Claude on ${r.optInt("activeDays")} days: ${r.optInt("prompts")} prompts in ${r.optInt("sessions")} chats.", T.body)
        val b = r.optJSONObject("busiest")
        if (b != null) Txt("Busiest day: " + runCatching { java.time.LocalDate.parse(b.getString("date")).format(java.time.format.DateTimeFormatter.ofPattern("EEEE")) }.getOrDefault(b.optString("date")) + (if (r.optString("favorite").isNotBlank()) " · mostly ${r.optString("favorite")}" else ""), T.small, muted = true)
        Txt("Worth about \$${"%,.0f".format(r.optDouble("cost"))} at API prices." + (if (r.optInt("streak") > 1) " Streak: ${r.optInt("streak")} days." else ""), T.small, muted = true)
    }
}

/** "Start with…": ready questions for a new chat, and the templates you saved. */
@Composable
fun StartIdeas(current: String, onPick: (String) -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var saving by remember { mutableStateOf(false) }
    var tplName by remember { mutableStateOf("") }
    val hub = remember { hubOf(c) }
    var saved by remember { mutableStateOf<List<Pair<String, String>>>(emptyList()) }
    LaunchedEffect(Unit) { if (hub != null) saved = runCatching { withContext(Dispatchers.IO) { Relay.templates(hub) } }.getOrDefault(emptyList()) }
    val guided = listOf(
        "Write an email" to "Help me write an email. Ask me who it is for and what I want to say, one question at a time.",
        "Plan a trip" to "Help me plan a trip. Ask me where, when, the budget and what I like, one question at a time, then make a day by day plan.",
        "Fix my computer" to "Something is wrong with my computer. Ask me what happens, step by step, and guide me to fix it in plain words.",
        "Explain an error" to "I will paste an error. Explain what it means in plain words and tell me how to fix it.",
        "Review my changes" to "Look at the changes in this project and tell me what is risky or can be better.",
    )
    Label("Start with")
    ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        (saved + guided).forEach { (n, t) -> Chip(n, false) { onPick(t) } }
        if (current.isNotBlank() && hub != null) Chip("Save this as a template", false) { saving = true }
    }
    if (saving) androidx.compose.material3.AlertDialog(onDismissRequest = { saving = false }, containerColor = MaterialTheme.colorScheme.surfaceVariant,
        title = { Txt("Save as a template", T.h2) },
        text = { Column { Field(tplName, { tplName = it.take(40) }, "Name, e.g. Review", selectAll = true); Spacer(Modifier.height(6.dp)); Txt("Your message is kept as a starting point for new chats on this server.", T.small, muted = true) } },
        confirmButton = { TextAction("Save") { val n = tplName.trim(); if (n.isNotEmpty() && hub != null) scope.launch { runCatching { withContext(Dispatchers.IO) { Relay.addTemplate(hub, n, current) } }.onSuccess { saved = saved.filter { it.first != n } + (n to current); Toast.makeText(c, "Template saved", Toast.LENGTH_SHORT).show() }.onFailure { Toast.makeText(c, "Could not save: " + (it.message ?: ""), Toast.LENGTH_LONG).show() } }; saving = false; tplName = "" } },
        dismissButton = { TextAction("Cancel") { saving = false } })
}

/** One line about the project of this chat: its chats, what they cost and what changed. */
@Composable
fun ProjectLine(m: Machine, cwd: String) {
    if (cwd.isBlank()) return
    var line by remember(cwd) { mutableStateOf("") }
    LaunchedEffect(cwd) {
        line = runCatching { withContext(Dispatchers.IO) { JSONObject(Relay.get(m, "/projects/summary?cwd=" + java.net.URLEncoder.encode(cwd, "UTF-8"))) } }.getOrNull()?.let { o ->
            listOfNotNull("${o.optInt("chats")} chats", "$%.2f".format(o.optDouble("cost")) + " so far", if (o.isNull("changed")) null else "${o.optInt("changed")} changed file${if (o.optInt("changed") == 1) "" else "s"}", o.optString("branch").takeIf { it.isNotBlank() && !it.startsWith("No commits") }).joinToString(" · ")
        } ?: ""
    }
    if (line.isNotBlank()) Row(verticalAlignment = Alignment.CenterVertically) { Glyph("home", 16.dp); Spacer(Modifier.width(8.dp)); Txt(line, T.small, muted = true) }
}

/** One-tap tools for Claude: Memory, up-to-date docs, GitHub and more. */
@Composable
fun ToolCatalogCard() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    val hub = remember { hubOf(c) }
    var tools by remember { mutableStateOf<List<Triple<String, String, String>>?>(null) }
    var err by remember { mutableStateOf("") }
    var busy by remember { mutableStateOf("") }
    var asking by remember { mutableStateOf<Triple<String, String, String>?>(null) }
    var secret by remember { mutableStateOf("") }
    LaunchedEffect(Unit) { if (hub != null) runCatching { withContext(Dispatchers.IO) { Relay.mcpCatalog(hub) } }.onSuccess { tools = it }.onFailure { err = if ((it.message ?: "").contains("404")) "This server is too old for this. Update it first." else friendlyError(it, hub.name).substringBefore(" Your message") } }
    fun add(id: String, secret: String) { if (hub == null) return; busy = id; scope.launch { runCatching { withContext(Dispatchers.IO) { Relay.mcpInstall(hub, id, secret) } }.onSuccess { Toast.makeText(c, it, Toast.LENGTH_LONG).show(); err = "" }.onFailure { err = it.message ?: "Could not add it." }; busy = "" } }
    Fold("plug", "Add a tool for Claude", "Ready-made tools that you add with one tap. New chats can use them. Some need a key from the service.") {
        Spacer(Modifier.height(8.dp))
        if (err.isNotBlank()) Txt(err, T.small, color = MaterialTheme.colorScheme.primary)
        if (tools == null && err.isBlank()) Skeleton(48.dp, radius = 14.dp)
        tools?.forEach { t ->
            val (name, about) = t.second.split("|", limit = 2).let { it[0] to it.getOrElse(1) { "" } }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f).padding(vertical = 6.dp)) { Txt(name, T.body.copy(fontWeight = FontWeight.SemiBold)); Txt(about, T.small, muted = true) }
                TextAction(if (busy == t.first) "Adding…" else "Add") { if (busy.isEmpty()) { if (t.third.isNotBlank()) { asking = t; secret = "" } else add(t.first, "") } }
            }
        }
    }
    asking?.let { t -> androidx.compose.material3.AlertDialog(onDismissRequest = { asking = null }, containerColor = MaterialTheme.colorScheme.surfaceVariant,
        title = { Txt(t.second.substringBefore("|"), T.h2) },
        text = { Column { Txt("This tool needs ${t.third}. It is kept on your server, in the settings of Claude Code.", T.small, muted = true); Spacer(Modifier.height(8.dp)); Field(secret, { secret = it.trim() }, "Paste it here", mono = true) } },
        confirmButton = { TextAction("Add") { val s = secret; asking = null; add(t.first, s) } }, dismissButton = { TextAction("Cancel") { asking = null } }) }
}

/** What is different in the project right now (git): the files, and the lines that changed. */
@Composable
fun ChangesDialog(m: Machine, sid: String, onUndo: () -> Unit, onClose: () -> Unit) {
    var data by remember { mutableStateOf<JSONObject?>(null) }
    var err by remember { mutableStateOf("") }
    LaunchedEffect(sid) {
        runCatching { withContext(Dispatchers.IO) { JSONObject(Relay.get(m, "/sessions/$sid/changes")) } }
            .onSuccess { data = it }.onFailure { err = if ((it.message ?: "").contains("404")) "This server is too old for this. Update it first." else "Could not read the changes." }
    }
    val cs = MaterialTheme.colorScheme
    androidx.compose.material3.AlertDialog(onDismissRequest = onClose, containerColor = cs.surfaceVariant,
        title = { Txt("What changed", T.h2) },
        text = {
            androidx.compose.foundation.text.selection.DisableSelection {
                Column(Modifier.heightIn(max = 420.dp).verticalScroll(androidx.compose.foundation.rememberScrollState())) {
                    val d = data
                    if (err.isNotBlank()) Txt(err, T.small, color = cs.primary)
                    else if (d == null) Skeleton(60.dp)
                    else if (!d.optBoolean("git")) Txt("This project does not use git, so there is nothing to compare with. You can still undo the last run.", T.small)
                    else {
                        val files = d.optJSONArray("files") ?: org.json.JSONArray()
                        if (files.length() == 0) Txt("Nothing is different from the last commit.", T.small)
                        for (i in 0 until files.length()) { val f = files.getJSONObject(i); Txt((when (f.optString("status")) { "new" -> "+ "; "deleted" -> "− "; else -> "~ " }) + f.optString("path"), T.small) }
                        val diff = d.optString("diff")
                        if (diff.isNotBlank()) {
                            Spacer(Modifier.height(10.dp))
                            diff.lines().take(400).forEach { l ->
                                val col = if (l.startsWith("+") && !l.startsWith("+++")) androidx.compose.ui.graphics.Color(0xFF4CAF7D) else if (l.startsWith("-") && !l.startsWith("---")) cs.primary else if (l.startsWith("@@")) cs.outline else cs.onSurface
                                androidx.compose.material3.Text(l, color = col, style = T.small.copy(fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace, fontSize = 11.sp, lineHeight = 15.sp))
                            }
                        }
                    }
                }
            }
        },
        confirmButton = { TextAction("Close") { onClose() } },
        dismissButton = { TextAction("Undo the last run") { onUndo() } })
}
