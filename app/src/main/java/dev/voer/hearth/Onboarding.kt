package dev.voer.hearth

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The first-run tutorial. Every step can be skipped; the same steps are reachable later from Settings. */
@Composable
fun Welcome(finish: () -> Unit) {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var step by remember { mutableIntStateOf(0) }
    var connected by remember { mutableStateOf(Store.machines(c).isNotEmpty()) }
    var found by remember { mutableStateOf<List<Machine>>(emptyList()) }
    var scanning by remember { mutableStateOf(false) }
    val last = 3
    Column(Modifier.fillMaxSize().systemBarsPadding().padding(24.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp, Alignment.CenterHorizontally)) {
            repeat(last + 1) { i ->
                Box(Modifier.height(6.dp).width(if (i == step) 22.dp else 6.dp).clip(RoundedCornerShape(50))
                    .background(if (i <= step) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline))
            }
        }
        Spacer(Modifier.height(24.dp))
        androidx.compose.animation.AnimatedContent(step, Modifier.weight(1f), transitionSpec = {
            val dir = if (targetState > initialState) 1 else -1
            androidx.compose.animation.ContentTransform(
                targetContentEnter = androidx.compose.animation.slideInHorizontally(androidx.compose.animation.core.tween(320)) { it / 5 * dir } + androidx.compose.animation.fadeIn(androidx.compose.animation.core.tween(320)),
                initialContentExit = androidx.compose.animation.slideOutHorizontally(androidx.compose.animation.core.tween(220)) { -it / 5 * dir } + androidx.compose.animation.fadeOut(androidx.compose.animation.core.tween(160)),
            )
        }, label = "step") { step -> Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {
            when (step) {
                0 -> {
                    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Spark(84.dp) }
                    Spacer(Modifier.height(20.dp))
                    Txt("Welcome to Hearth", T.title)
                    Spacer(Modifier.height(10.dp))
                    Txt("Talk to Claude Code from your phone. Your chats and projects stay the same on your computer and your phone.", T.body, muted = true)
                    Spacer(Modifier.height(16.dp))
                    listOf("Start and continue chats from anywhere", "Get a message, or a call, when Claude is done or needs you", "See how much of your Claude limit is left, also as a widget").forEach {
                        Txt("•  $it", T.body); Spacer(Modifier.height(6.dp))
                    }
                }
                1 -> {
                    Txt("First, set up your server", T.title)
                    Spacer(Modifier.height(10.dp))
                    Txt("A server is a computer that is always on, far away, like a robot that never sleeps. Hearth lives there, so your phone works even when your laptop is closed.", T.body, muted = true)
                    Spacer(Modifier.height(12.dp))
                    listOf("On your server, open a terminal (for example with ssh).", "Paste this line and press Enter. It asks a few easy questions.", "It shows an address and a 6-digit code. Type them below.").forEachIndexed { i, t ->
                        Txt("${i + 1}.  $t", T.body); Spacer(Modifier.height(6.dp))
                    }
                    Spacer(Modifier.height(4.dp))
                    val cmd = "curl -fsSL https://raw.githubusercontent.com/veelvoer/hearth/main/install.sh | bash"
                    Card {
                        Txt(cmd, T.small.copy(fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace))
                        Spacer(Modifier.height(6.dp))
                        TextAction("Copy the line") {
                            (c.getSystemService(android.content.Context.CLIPBOARD_SERVICE) as android.content.ClipboardManager).setPrimaryClip(android.content.ClipData.newPlainText("Hearth", cmd))
                            Toast.makeText(c, "Copied. Paste it on your server.", Toast.LENGTH_SHORT).show()
                        }
                    }
                    Spacer(Modifier.height(12.dp))
                    if (connected) {
                        Card { Txt("✓ Connected: ${Store.machines(c).joinToString { it.name }}", T.body) }
                    } else {
                        androidx.compose.runtime.key(found.firstOrNull()?.host) { AddComputer(found.firstOrNull()) { m ->
                            if (m != null) {
                                val had = Store.machines(c)
                                Store.saveMachines(c, had.filter { it.host != m.host } + m)
                                if (had.isEmpty()) Store.setCallMe(c, true)
                                CallService.sync(c); Ui.machinesRev++; connected = true
                                Toast.makeText(c, "Connected to ${m.name}", Toast.LENGTH_SHORT).show()
                            }
                        } }
                        Spacer(Modifier.height(8.dp))
                        Txt("No server? You can connect straight to your computer when both are on the same Wi-Fi. Open Hearth on the computer: it shows an address and a code.", T.small, muted = true)
                        TextAction(if (scanning) "Looking…" else "Find my computer on this Wi-Fi") { if (!scanning) scope.launch { scanning = true; found = withContext(Dispatchers.IO) { Relay.discover() }; scanning = false; if (found.isEmpty()) Toast.makeText(c, "Nothing found. Type the address from the computer.", Toast.LENGTH_LONG).show() } }
                    }
                }
                2 -> {
                    Txt("Stay in the loop", T.title)
                    Spacer(Modifier.height(10.dp))
                    Txt("Allow notifications so Hearth can tell you when Claude is finished or waiting for you. You can also get a phone call for the important moments; choose that per chat, inside the chat.", T.body, muted = true)
                    Spacer(Modifier.height(14.dp))
                    Txt("Hearth will ask Android for permission when you finish this tutorial.", T.small, muted = true)
                }
                else -> {
                    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Spark(84.dp) }
                    Spacer(Modifier.height(20.dp))
                    Txt("You're all set", T.title)
                    Spacer(Modifier.height(10.dp))
                    listOf("Tap New session, pick a project and say what you want.", "A project you start here is made on your server. If your laptop is off, Hearth offers to install it there the next time you open it.", "Choose Coding to build things, or Talk for a simple conversation.", "Use the options inside a chat to change the model or how much effort Claude puts in.", "You can replay this tutorial from Settings.").forEach {
                        Txt("•  $it", T.body); Spacer(Modifier.height(6.dp))
                    }
                    Spacer(Modifier.height(8.dp))
                    Txt("Hearth is an independent app and is not made by or affiliated with Anthropic. “Claude” is a trademark of Anthropic.", T.small, muted = true)
                }
            }
        } }
        Spacer(Modifier.height(12.dp))
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            if (step in 1 until last) { TextAction("Back") { step -= 1 }; Spacer(Modifier.width(20.dp)) }
            if (step < last) TextAction("Skip tutorial") { finish() }
            Spacer(Modifier.weight(1f))
            Button(when (step) { 0 -> "Get started"; last -> "Open Hearth"; 1 -> if (connected) "Continue" else "Later"; else -> "Continue" }) { if (step == last) finish() else step += 1 }
        }
    }
}
