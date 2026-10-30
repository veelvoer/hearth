package dev.voer.hearth

import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.border
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.luminance
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        Store.purgeOldLogin(this)
        Ui.load(this)
        handle(intent)
        setContent { ClaudeTheme { Root() } }
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    private val notifPerm = registerForActivityResult(androidx.activity.result.contract.ActivityResultContracts.RequestPermission()) {}
    private var askedNotif = false

    fun askNotifications() {
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED) { askedNotif = true; notifPerm.launch(android.Manifest.permission.POST_NOTIFICATIONS) }
    }

    private fun handle(i: android.content.Intent?) {
        val link = i?.data
        if (link != null && link.scheme == "hearth" && link.host == "share") { Ui.pendingJoin = link.toString(); Ui.tab = 0; Ui.shareRev++; return }
        if (link != null && (link.scheme == "hearth" || link.scheme == "hearth")) {   // pairing link: connect to the server it describes
            parseMachine(link.toString(), "", "")?.let { m ->
                val had = Store.machines(this)
                Store.saveMachines(this, had.filter { it.host != m.host } + m)
                if (had.isEmpty()) Store.setCallMe(this, true)
                CallService.sync(this); Ui.machinesRev++; Ui.tab = 0
                android.widget.Toast.makeText(this, "Connected to ${m.name}", android.widget.Toast.LENGTH_LONG).show()
            }
            return
        }
        val host = i?.getStringExtra("open_host") ?: return
        Ui.pendingOpen = PendingOpen(host, i.getStringExtra("open_session") ?: return, i.getStringExtra("open_title") ?: "Session", i.getStringExtra("open_cwd") ?: "", i.getBooleanExtra("open_voice", false))
        Ui.tab = 0
    }

    private var liveJob: kotlinx.coroutines.Job? = null

    override fun onPause() { liveJob?.cancel(); super.onPause() }

    /** While the app is open the numbers follow the computer: read them every 20 s, and when they are over 5 minutes old ask the computer to read the limits again (at most every 5 minutes). */
    private fun keepUsageLive() {
        liveJob?.cancel()
        liveJob = lifecycleScope.launch {
            var asked = 0L
            while (true) {
                if (Store.signedIn(this@MainActivity)) {
                    val now = System.currentTimeMillis()
                    val stale = (Store.usage(this@MainActivity)?.at ?: 0L) < now - 5 * 60_000L
                    val ask = stale && now - asked > 5 * 60_000L
                    if (ask) asked = now
                    Ui.refresh(this@MainActivity, ask = ask)
                }
                kotlinx.coroutines.delay(20_000)
            }
        }
    }

    override fun onResume() {
        super.onResume()
        Ui.load(this)
        keepUsageLive()
        if (Build.VERSION.SDK_INT >= 33 && !askedNotif && Store.signedIn(this) &&
            checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED) { askedNotif = true; notifPerm.launch(android.Manifest.permission.POST_NOTIFICATIONS) }
        if (Store.signedIn(this)) {
            Sched.ensure(this)
            CallService.sync(this)
            if (System.currentTimeMillis() - Updates.checkedAt > 6 * 3600_000L) lifecycleScope.launch { Updates.check(this@MainActivity) }
        }
    }
}

@Composable
private fun Root() {
    val c = androidx.compose.ui.platform.LocalContext.current
    var tour by remember { mutableStateOf(!Store.onboarded(c)) }
    val seenTour = remember { Ui.tourRev }   // "Show tutorial" in Settings counts up; a new screen must not replay an old press
    LaunchedEffect(Ui.tourRev) { if (Ui.tourRev != seenTour) tour = true }
    LaunchedEffect(tour) { if (tour) Store.setOnboarded(c, true) }   // the tutorial shows once; closing the app halfway must not bring it back
    var news by remember { mutableStateOf<List<Pair<String, List<String>>>>(emptyList()) }
    val wasOnboarded = remember { Store.onboarded(c) }   // read before the tutorial marks it
    LaunchedEffect(Unit) { news = Updates.whatsNew(c, wasOnboarded) }
    if (news.isNotEmpty()) {
        androidx.compose.material3.AlertDialog(
            onDismissRequest = { news = emptyList() },
            title = { Txt("Updated to Hearth ${Updates.current(c)}", T.h2) },
            text = {
                Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Txt(if (news.size > 1) "Everything that changed since the version you had." else "What changed in this version.", T.small, muted = true)
                    news.forEach { (v, lines) -> Spacer(Modifier.height(8.dp)); Txt("Hearth $v", T.body.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold)); lines.forEach { l -> Txt(l, T.small) } }
                }
            },
            confirmButton = { TextAction("Got it") { news = emptyList() } },
        )
    }
    Box(Modifier.fillMaxSize().glow()) {
        if (tour) Welcome { Store.setOnboarded(c, true); tour = false; (c as? MainActivity)?.askNotifications(); if (it) Tour.start() } else Main()
        TourOverlay()
    }
}

@Composable
private fun Main() {
    var tab = Ui.tab
    val tabs = listOf("Chats", "Home", "History", "Settings")
    Column(Modifier.fillMaxSize().systemBarsPadding()) {
        Updates.release?.let { r -> androidx.compose.animation.AnimatedVisibility(!Ui.chatOpen && tab != 3, enter = androidx.compose.animation.expandVertically() + androidx.compose.animation.fadeIn(), exit = androidx.compose.animation.shrinkVertically() + androidx.compose.animation.fadeOut()) { Row(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.primary).clickable { Ui.tab = 3 }.padding(horizontal = 20.dp, vertical = 10.dp)) {
            Txt("Hearth ${r.version} is ready", T.small, color = MaterialTheme.colorScheme.onPrimary, modifier = Modifier.weight(1f))
            Txt("Update", T.small.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold), color = MaterialTheme.colorScheme.onPrimary)
        } } }
        if (CallSession.active && !Ui.chatOpen) Row(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.primary)
            .clickable { Ui.pendingOpen = PendingOpen(CallSession.host, CallSession.sessId, CallSession.title, "", true); Ui.tab = 0 }.padding(horizontal = 20.dp, vertical = 10.dp)) {
            Txt("Call with Claude · tap to return", T.small, color = MaterialTheme.colorScheme.onPrimary, modifier = Modifier.weight(1f))
            Txt("End", T.small.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold), color = MaterialTheme.colorScheme.onPrimary, modifier = Modifier.clickable { CallSession.end() })
        }
        Box(Modifier.weight(1f)) {
            androidx.compose.animation.Crossfade(targetState = tab, animationSpec = androidx.compose.animation.core.tween(if (Fx.motion) 240 else 0), label = "tab") { t -> when (t) { 0 -> SessionsScreen(); 1 -> UsageScreen(); 2 -> HistoryScreen(); else -> SettingsScreen() } }
        }
        if (!Ui.chatOpen) {
            val cs = MaterialTheme.colorScheme
            val active = if (cs.background.luminance() < .5f) androidx.compose.ui.graphics.Color(0xFF2E2A26) else androidx.compose.ui.graphics.Color(0xFFE9E2D2)
            val bar = androidx.compose.foundation.shape.RoundedCornerShape(28.dp)
            Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 12.dp, bottom = 10.dp, top = 4.dp).tourTarget("tabs").panel(bar).padding(6.dp)) {
                tabs.forEachIndexed { i, s ->
                    Box(Modifier.weight(1f).clip(androidx.compose.foundation.shape.RoundedCornerShape(22.dp)).background(if (tab == i) active else androidx.compose.ui.graphics.Color.Transparent).clickable { Ui.tab = i }.padding(vertical = 8.dp), contentAlignment = Alignment.Center) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally) {
                            Glyph(listOf("chat", "home", "history", "gear")[i], 22.dp, if (tab == i) cs.primary else cs.onSurfaceVariant)
                            Txt(s, T.small.copy(fontSize = 11.sp, fontWeight = if (tab == i) androidx.compose.ui.text.font.FontWeight.SemiBold else androidx.compose.ui.text.font.FontWeight.Medium), muted = tab != i, color = if (tab == i) cs.primary else androidx.compose.ui.graphics.Color.Unspecified)
                        }
                    }
                }
            }
        }
    }
}
