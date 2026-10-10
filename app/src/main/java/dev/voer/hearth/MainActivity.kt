package dev.voer.hearth

import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
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

    override fun onResume() {
        super.onResume()
        Ui.load(this)
        if (Build.VERSION.SDK_INT >= 33 && !askedNotif && Store.signedIn(this) &&
            checkSelfPermission(android.Manifest.permission.POST_NOTIFICATIONS) != android.content.pm.PackageManager.PERMISSION_GRANTED) { askedNotif = true; notifPerm.launch(android.Manifest.permission.POST_NOTIFICATIONS) }
        if (Store.signedIn(this)) {
            Sched.ensure(this)
            CallService.sync(this)
            lifecycleScope.launch { Ui.refresh(this@MainActivity) }
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
    Box(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
        if (tour) Welcome { Store.setOnboarded(c, true); tour = false; (c as? MainActivity)?.askNotifications() } else Main()
    }
}

@Composable
private fun Main() {
    var tab = Ui.tab
    val tabs = listOf("Sessions", "Usage", "Settings")
    Column(Modifier.fillMaxSize().systemBarsPadding()) {
        Updates.release?.let { r -> androidx.compose.animation.AnimatedVisibility(!Ui.chatOpen && tab != 2, enter = androidx.compose.animation.expandVertically() + androidx.compose.animation.fadeIn(), exit = androidx.compose.animation.shrinkVertically() + androidx.compose.animation.fadeOut()) { Row(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.primary).clickable { Ui.tab = 2 }.padding(horizontal = 20.dp, vertical = 10.dp)) {
            Txt("Hearth ${r.version} is ready", T.small, color = MaterialTheme.colorScheme.onPrimary, modifier = Modifier.weight(1f))
            Txt("Update", T.small.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold), color = MaterialTheme.colorScheme.onPrimary)
        } } }
        if (CallSession.active && !Ui.chatOpen) Row(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.primary)
            .clickable { Ui.pendingOpen = PendingOpen(CallSession.host, CallSession.sessId, CallSession.title, "", true); Ui.tab = 0 }.padding(horizontal = 20.dp, vertical = 10.dp)) {
            Txt("Call with Claude · tap to return", T.small, color = MaterialTheme.colorScheme.onPrimary, modifier = Modifier.weight(1f))
            Txt("End", T.small.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold), color = MaterialTheme.colorScheme.onPrimary, modifier = Modifier.clickable { CallSession.end() })
        }
        Box(Modifier.weight(1f)) {
            androidx.compose.animation.Crossfade(targetState = tab, animationSpec = androidx.compose.animation.core.tween(240), label = "tab") { t -> when (t) { 0 -> SessionsScreen(); 1 -> UsageScreen(); else -> SettingsScreen() } }
        }
        if (!Ui.chatOpen) Box(Modifier.fillMaxWidth().height(1.dp).background(MaterialTheme.colorScheme.outline))
        if (!Ui.chatOpen) Row(Modifier.fillMaxWidth()) {
            tabs.forEachIndexed { i, s ->
                Column(Modifier.weight(1f).clickable { Ui.tab = i }.padding(top = 10.dp, bottom = 12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                    Txt(s, T.small.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Medium), muted = tab != i,
                        color = if (tab == i) MaterialTheme.colorScheme.primary else androidx.compose.ui.graphics.Color.Unspecified)
                    Spacer(Modifier.height(5.dp))
                    Box(Modifier.size(width = 18.dp, height = 2.dp).background(if (tab == i) MaterialTheme.colorScheme.primary else androidx.compose.ui.graphics.Color.Transparent))
                }
            }
        }
    }
}
