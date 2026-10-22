package dev.voer.hearth

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
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

/** One step of a workflow: what Claude does, and for how many minutes (0 = one run). */
class WfStep(var name: String, var prompt: String, var minutes: Int)

private val BUG_TEST = WfStep("Bug test", "Test this project like a careful user. Run it, go through every screen and feature, try odd input, and look for bugs, crashes and confusing things. Write every bug you find in a file BUGS.md in the project (what you did, what happened, what should have happened). Do not fix anything yet.", 120)
private val BUG_FIX = WfStep("Bug fix", "Read BUGS.md and fix every bug on it, one by one. After every fix, build the project and test the build. If a debug phone is connected (check with adb devices), install the build on it and test the fix there. Tick off each bug in BUGS.md when it is fixed and tested.", 180)
private val LIST_FIX = WfStep("Fix my bug list", "Call mcp__meter__bug_list and fix the open items one by one in the right project. After each fix, build and test it (on the debug phone if one is connected), then call mcp__meter__bug_fixed with the item's id.", 180)

/** Lets Claude work through steps by itself, each for a set time (one run stops after a while, a workflow keeps going). */
@Composable
fun WorkflowPanel(m: Machine, sid: String) {
    val scope = rememberCoroutineScope()
    var st by remember { mutableStateOf<JSONObject?>(null) }
    var msg by remember { mutableStateOf("") }
    var edit by remember { mutableStateOf(false) }
    fun load() { scope.launch { runCatching { withContext(Dispatchers.IO) { Relay.workflow(m, sid) } }.onSuccess { st = it }.onFailure { msg = "This server has no workflows yet. Update it first." } } }
    LaunchedEffect(sid) { while (sid.isNotEmpty()) { load(); delay(10_000) } }
    fun act(a: String) { scope.launch { runCatching { withContext(Dispatchers.IO) { Shares.postJson(m, "/sessions/$sid/workflow", JSONObject().put("action", a)) } }.onSuccess { st = it; msg = "" }.onFailure { msg = it.message ?: "Didn't work" } } }
    Label("Workflow")
    val s = st; val state = s?.optString("state") ?: "none"
    if (sid.isEmpty()) { Txt("Send a first message, then you can set up a workflow.", T.small, muted = true); return }
    if (state == "running" || state == "paused") {
        val steps = s!!.optJSONArray("steps") ?: JSONArray(); val left = if (s.isNull("minutesLeft")) -1 else s.optInt("minutesLeft", -1)
        Txt("Step ${s.optInt("idx") + 1} of ${steps.length()}: ${s.optString("step")}" + (if (state == "paused") " · paused" else if (left >= 0) " · ${if (left >= 60) "${left / 60}h ${left % 60}m" else "${left}m"} left" else ""), T.body)
        if (s.optString("note").isNotEmpty()) Txt(s.optString("note"), T.small, color = MaterialTheme.colorScheme.error)
        ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            if (state == "running") Chip("Pause", false) { act("pause") } else Chip("Resume", false) { act("resume") }
            Chip("Skip step", false) { act("skip") }
            Chip("Stop", false) { act("stop") }
        }
    } else {
        Txt(if (state == "done") "The last workflow finished." else "Let Claude work through steps by itself, each for as long as you choose.", T.small, muted = true)
        ChipRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Chip("Set up a workflow", false) { edit = true } }
    }
    if (msg.isNotEmpty()) Txt(msg, T.small, color = MaterialTheme.colorScheme.error)
    if (edit) WorkflowEditor(onClose = { edit = false }) { steps, auto ->
        scope.launch {
            val body = JSONObject().put("auto", auto).put("steps", JSONArray(steps.map { JSONObject().put("name", it.name).put("prompt", it.prompt).put("minutes", it.minutes) }))
            runCatching { withContext(Dispatchers.IO) { Shares.postJson(m, "/sessions/$sid/workflow", body) } }
                .onSuccess { st = it; msg = if (it.optBoolean("autoBlocked")) "Started. Full auto is off on this server, so Claude may wait for your OK." else ""; edit = false }
                .onFailure { msg = it.message ?: "Couldn't start" }
        }
    }
}

@Composable
private fun WorkflowEditor(onClose: () -> Unit, start: (List<WfStep>, Boolean) -> Unit) {
    val steps = remember { mutableStateListOf(WfStep(BUG_TEST.name, BUG_TEST.prompt, BUG_TEST.minutes), WfStep(BUG_FIX.name, BUG_FIX.prompt, BUG_FIX.minutes)) }
    var auto by remember { mutableStateOf(false) }
    AlertDialog(onDismissRequest = onClose, containerColor = MaterialTheme.colorScheme.surfaceVariant,
        title = { Txt("Workflow", T.h2) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Txt("Steps run one after another. Claude keeps working on a step for its time, also when one run ends early.", T.small, muted = true)
                steps.forEachIndexed { i, x ->
                    Txt("Step ${i + 1}", T.small.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold))
                    var name by remember(x) { mutableStateOf(x.name) }; var prompt by remember(x) { mutableStateOf(x.prompt) }; var min by remember(x) { mutableStateOf(x.minutes.toString()) }
                    Field(name, { name = it; x.name = it }, "Name")
                    Field(min, { v -> min = v.filter { it.isDigit() }.take(4); x.minutes = min.toIntOrNull() ?: 0 }, "Minutes (0 = one run)", number = true)
                    Field(prompt, { prompt = it; x.prompt = it }, "What should Claude do?", singleLine = false)
                    if (steps.size > 1) TextAction("Remove step") { steps.remove(x) }
                }
                Txt("Add a step", T.small, muted = true)
                ChipRow(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    listOf(BUG_TEST, BUG_FIX, LIST_FIX).forEach { p -> Chip(p.name, false) { if (steps.size < 10) steps.add(WfStep(p.name, p.prompt, p.minutes)) } }
                    Chip("Custom", false) { if (steps.size < 10) steps.add(WfStep("Step ${steps.size + 1}", "", 60)) }
                }
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f).padding(end = 12.dp)) { Txt("Don't ask for permission", T.body); Txt("Needs Full auto allowed on the server. Otherwise Claude may wait for your OK.", T.small, muted = true) }
                    Switch(auto, { auto = it })
                }
            }
        },
        confirmButton = { Button("Start") { start(steps.filter { it.prompt.isNotBlank() }, auto) } },
        dismissButton = { TextAction("Cancel", onClose) })
}
