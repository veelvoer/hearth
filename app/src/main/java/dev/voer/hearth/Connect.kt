package dev.voer.hearth

import androidx.compose.foundation.layout.*
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** Computers on this Wi-Fi, found by themselves. Tap one: it asks the person at the computer to accept. */
@Composable
fun FindComputers(onConnected: (List<Machine>) -> Unit) {
    val scope = rememberCoroutineScope()
    var found by remember { mutableStateOf<List<Machine>?>(null) }
    var scanning by remember { mutableStateOf(false) }
    var waiting by remember { mutableStateOf<Machine?>(null) }
    var msg by remember { mutableStateOf("") }
    fun scan() { scope.launch { scanning = true; msg = ""; found = withContext(Dispatchers.IO) { Relay.discover() }; scanning = false } }
    LaunchedEffect(Unit) { scan() }
    fun ask(m: Machine) {
        scope.launch {
            waiting = m; msg = ""
            val device = (android.os.Build.MANUFACTURER.replaceFirstChar { it.uppercase() } + " " + android.os.Build.MODEL).trim()
            val r = withContext(Dispatchers.IO) {
                runCatching {
                    val id = Relay.askToConnect(m, device)
                    var out = JSONObject().put("status", "expired")
                    for (i in 0 until 80) {
                        delay(1500)
                        val st = Relay.askStatus(m, id)
                        if (st.optString("status") != "pending") { out = st; break }
                    }
                    out
                }
            }
            waiting = null
            r.onSuccess { st ->
                when (st.optString("status")) {
                    "accepted" -> {
                        val list = mutableListOf(Machine(st.optString("name", m.name), m.host, m.port, st.getString("token")))
                        st.optJSONObject("server")?.let { s -> parseMachine(s.getString("url"), s.optString("name", "Server"), s.getString("token"))?.let { list.add(it) } }
                        onConnected(list)
                    }
                    "declined" -> msg = "${m.name} said no. You can try again."
                    else -> msg = "Nobody answered on ${m.name}. Open Hearth on the computer, then try again."
                }
            }.onFailure { msg = (it as? ApiException)?.message ?: "Could not reach ${m.name}." }
        }
    }
    Card {
        Label("Computers on this Wi-Fi")
        Spacer(Modifier.height(8.dp))
        val w = waiting
        when {
            w != null -> Row(verticalAlignment = Alignment.CenterVertically) {
                CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.primary)
                Spacer(Modifier.width(12.dp))
                Txt("Waiting for ${w.name}… press Accept on the computer.", T.body)
            }
            scanning && found.isNullOrEmpty() -> Row(verticalAlignment = Alignment.CenterVertically) {
                CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp, color = MaterialTheme.colorScheme.primary)
                Spacer(Modifier.width(12.dp)); Txt("Looking…", T.body, muted = true)
            }
            found.isNullOrEmpty() -> Txt("No computer found. Open Hearth on your computer and make sure both are on the same Wi-Fi.", T.small, muted = true)
            else -> found!!.forEach { m ->
                Row(Modifier.fillMaxWidth().padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) { Txt(m.name, T.body); Txt("On this Wi-Fi", T.small, muted = true) }
                    Button("Connect", Modifier) { ask(m) }
                }
            }
        }
        if (msg.isNotBlank()) { Spacer(Modifier.height(6.dp)); Txt(msg, T.small, color = MaterialTheme.colorScheme.primary) }
        if (waiting == null) { Spacer(Modifier.height(4.dp)); TextAction(if (scanning) "Looking…" else "Look again") { if (!scanning) scan() } }
    }
}
