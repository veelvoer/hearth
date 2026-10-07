package dev.voer.hearth

import android.app.*
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.AudioAttributes
import android.media.RingtoneManager
import android.os.Bundle
import android.os.IBinder
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import org.json.JSONObject
import kotlin.math.max

data class CallEvent(val id: Long, val kind: String, val session: String, val title: String, val cwd: String,
                     val text: String, val seconds: Int, val req: String, val machine: String, val call: Boolean = true, val boot: String = "",
                     val question: String = "", val options: List<String> = emptyList(), val multi: Boolean = false)

object CallApi {
    private fun post(m: Machine, path: String, body: JSONObject) {
        val c = Relay.open(m, path, post = true)
        try { c.outputStream.use { it.write(body.toString().toByteArray()) }; c.responseCode } finally { c.disconnect() }
    }

    fun setAway(m: Machine, on: Boolean) = post(m, "/away", JSONObject().put("on", on))
    fun decide(m: Machine, req: String, behavior: String) = post(m, "/decision", JSONObject().put("req", req).put("behavior", behavior))

    /** Blocks while connected to the relay's event stream; returns when it drops or [active] turns false. */
    fun listen(m: Machine, active: () -> Boolean, onEvent: (CallEvent) -> Unit) {
        val c = Relay.open(m, "/events", read = 45_000)
        try {
            if (c.responseCode != 200) throw ApiException(c.responseCode, "Relay error ${c.responseCode}")
            c.inputStream.bufferedReader().use { r ->
                while (active()) {
                    val line = r.readLine() ?: break
                    if (!line.startsWith("data: ")) continue
                    val o = JSONObject(line.removePrefix("data: "))
                    onEvent(CallEvent(o.getLong("id"), o.getString("kind"), o.optString("session"), o.optString("title"),
                        o.optString("cwd"), o.optString("text"), o.optInt("seconds"), o.optString("req"), o.optString("machine"),
                        o.optBoolean("call", true), o.optString("boot"), o.optString("question"),
                        o.optJSONArray("options")?.let { op -> List(op.length()) { i -> op.getJSONObject(i).optString("label") } } ?: emptyList(), o.optBoolean("multi")))
                }
            }
        } finally { c.disconnect() }
    }
}

object Calls {
    private const val CALL = "call"
    private const val LISTEN = "listen"
    private const val MSG = "messages"

    fun channels(c: Context) {
        val nm = c.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(LISTEN, "Listening for calls", NotificationManager.IMPORTANCE_MIN))
        nm.createNotificationChannel(NotificationChannel(MSG, "Messages from Claude", NotificationManager.IMPORTANCE_DEFAULT))
        nm.createNotificationChannel(NotificationChannel(CALL, "Claude calls", NotificationManager.IMPORTANCE_HIGH).apply {
            setSound(RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE),
                AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE).setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION).build())
            enableVibration(true); vibrationPattern = longArrayOf(0, 600, 400, 600, 400, 600)
            lockscreenVisibility = Notification.VISIBILITY_PUBLIC
        })
    }

    fun listening(c: Context): Notification = NotificationCompat.Builder(c, LISTEN).setSmallIcon(R.drawable.ic_stat)
        .setContentTitle("Hearth").setContentText("Keeping your widgets and Claude updates live")
        .setOngoing(true).setSilent(true).build()

    /** Every milestone arrives as a message; only the ones you asked for ring like a call. */
    fun deliver(c: Context, m: Machine, e: CallEvent) {
        if (e.kind == "resolved") { c.getSystemService(NotificationManager::class.java).cancel(2000 + (e.req.hashCode() and 0xFFFF)); return }
        if (e.call) ring(c, m, e) else message(c, m, e)
    }

    private fun project(cwd: String) = cwd.trimEnd('/').substringAfterLast('/').ifEmpty { "Claude" }

    fun message(c: Context, m: Machine, e: CallEvent) {
        channels(c)
        val nid = 2000 + ((if (e.kind == "permission") e.req.hashCode() else e.session.hashCode()) and 0xFFFF)
        val fl = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        val open = PendingIntent.getActivity(c, nid, Intent(c, MainActivity::class.java).putExtra("open_host", m.host).putExtra("open_session", e.session)
            .putExtra("open_title", e.title).putExtra("open_cwd", e.cwd).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP), fl)
        val title = when (e.kind) {
            "permission" -> "${project(e.cwd)}: needs your OK"
            "question" -> "${project(e.cwd)}: Claude has a question"
            "error" -> "${project(e.cwd)}: something went wrong"
            "queued" -> "${project(e.cwd)}: waiting for your laptop"
            "started" -> "${project(e.cwd)}: your laptop started"
            else -> "${project(e.cwd)}: Claude is done"
        }
        val b = NotificationCompat.Builder(c, MSG).setSmallIcon(R.drawable.ic_stat).setContentTitle(title)
            .setContentText(e.text.ifEmpty { e.title }).setStyle(NotificationCompat.BigTextStyle().bigText(e.text.ifEmpty { e.title }))
            .setContentIntent(open).setAutoCancel(true).setPriority(NotificationCompat.PRIORITY_DEFAULT)
        if (e.kind == "permission" && e.req.isNotEmpty()) {
            fun act(behavior: String, label: String, code: Int) = NotificationCompat.Action.Builder(0, label, PendingIntent.getBroadcast(c, nid + code,
                Intent(c, CallReceiver::class.java).setAction("decide").putExtra("host", m.host).putExtra("req", e.req).putExtra("behavior", behavior).putExtra("nid", nid), fl)).build()
            b.addAction(act("allow", "Allow", 1)).addAction(act("deny", "Deny", 2))
        }
        runCatching { c.getSystemService(NotificationManager::class.java).notify(nid, b.build()) }
    }

    private fun extras(i: Intent, m: Machine, e: CallEvent, nid: Int) = i.putExtra("host", m.host).putExtra("kind", e.kind)
        .putExtra("title", e.title).putExtra("text", e.text).putExtra("session", e.session).putExtra("cwd", e.cwd)
        .putExtra("req", e.req).putExtra("nid", nid)

    fun ring(c: Context, m: Machine, e: CallEvent) {
        channels(c)
        val nid = 100 + (e.id % 800).toInt()
        val fl = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        val answer = PendingIntent.getActivity(c, nid, extras(Intent(c, CallActivity::class.java), m, e, nid)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK), fl)
        val decline = PendingIntent.getBroadcast(c, nid, extras(Intent(c, CallReceiver::class.java), m, e, nid), fl)
        val what = when (e.kind) { "permission" -> "needs permission"; "question" -> "has a question"; else -> "is done" }
        val n = NotificationCompat.Builder(c, CALL).setSmallIcon(R.drawable.ic_stat)
            .setContentTitle("Claude $what").setContentText(e.text.ifEmpty { e.title })
            .setCategory(NotificationCompat.CATEGORY_CALL).setPriority(NotificationCompat.PRIORITY_MAX)
            .setOngoing(true).setTimeoutAfter(45_000).setFullScreenIntent(answer, true)
            .setStyle(NotificationCompat.CallStyle.forIncomingCall(Person.Builder().setName("Claude · ${e.title}").setImportant(true).build(), decline, answer))
            .build()
        n.flags = n.flags or Notification.FLAG_INSISTENT
        runCatching { c.getSystemService(NotificationManager::class.java).notify(nid, n) }
    }
}

/** Runs in the background: listens to your servers for Claude's messages and calls, and keeps widgets and usage live. */
class CallService : Service() {
    @Volatile private var gen = 0
    private var screen: BroadcastReceiver? = null
    private val lastSeen = HashMap<String, Long>()
    private val boot = HashMap<String, String>()
    override fun onBind(i: Intent?): IBinder? = null

    override fun onStartCommand(i: Intent?, f: Int, id: Int): Int {
        Calls.channels(this)
        ServiceCompat.startForeground(this, 1, Calls.listening(this), ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
        val g = ++gen
        if (Store.callMe(this)) Store.machines(this).forEach { m -> Thread { loop(m, g) }.apply { isDaemon = true; start() } }
        Thread { ticker(g) }.apply { isDaemon = true; start() }
        if (screen == null) {
            screen = object : BroadcastReceiver() {
                override fun onReceive(c: Context, i: Intent) { Thread { WidgetUpdater.updateAll(c); refreshIfStale(c, 2) }.start() }
            }
            ContextCompat.registerReceiver(this, screen, android.content.IntentFilter(Intent.ACTION_SCREEN_ON).apply { addAction(Intent.ACTION_USER_PRESENT) }, ContextCompat.RECEIVER_NOT_EXPORTED)
        }
        return START_STICKY
    }

    private fun loop(m: Machine, g: Int) {
        var wait = 3_000L
        while (g == gen) {
            runCatching {
                CallApi.setAway(m, true)
                wait = 3_000L
                CallApi.listen(m, { g == gen }) { e ->
                    if (e.boot != boot[m.host]) { boot[m.host] = e.boot; lastSeen[m.host] = 0 }
                    if (e.id > (lastSeen[m.host] ?: 0)) { lastSeen[m.host] = e.id; Calls.deliver(this, m, e) }
                }
            }
            if (g != gen) break
            Thread.sleep(wait); wait = (wait * 2).coerceAtMost(60_000)
        }
    }

    /** Every 30 s the widgets are redrawn (so countdowns are live); usage is refetched every few minutes. */
    private fun ticker(g: Int) {
        while (g == gen) {
            runCatching { WidgetUpdater.updateAll(this); refreshIfStale(this, 1) }
            Thread.sleep(30_000)
        }
    }

    private fun refreshIfStale(c: Context, minutes: Int) {
        if (!Store.signedIn(c)) return
        val u = Store.usage(c)
        if (u == null || System.currentTimeMillis() - u.at > minutes * 60_000L) runCatching { kotlinx.coroutines.runBlocking { Repo.refresh(c) } }
    }

    override fun onDestroy() { gen++; screen?.let { runCatching { unregisterReceiver(it) } }; screen = null; super.onDestroy() }

    companion object {
        /** Needed whenever there are widgets to keep live, or servers to listen to. */
        fun needed(c: Context) = (Store.callMe(c) && Store.machines(c).isNotEmpty()) || (Store.signedIn(c) && WidgetUpdater.hasWidgets(c))

        fun sync(c: Context) {
            val i = Intent(c, CallService::class.java)
            if (needed(c)) runCatching { ContextCompat.startForegroundService(c, i) } else c.stopService(i)
        }

        fun turnOff(c: Context) {
            val machines = Store.machines(c)
            Thread { machines.forEach { runCatching { CallApi.setAway(it, false) } } }.start()
            sync(c)
        }
    }
}

class CallReceiver : BroadcastReceiver() {
    override fun onReceive(c: Context, i: Intent) {
        c.getSystemService(NotificationManager::class.java).cancel(i.getIntExtra("nid", 0))
        val m = Store.machines(c).firstOrNull { it.host == i.getStringExtra("host") } ?: return
        val req = i.getStringExtra("req") ?: return
        val behavior = when {
            i.action == "decide" -> i.getStringExtra("behavior") ?: return
            i.getStringExtra("kind") == "permission" -> "deny"   // declining a permission call denies it
            else -> return
        }
        val pr = goAsync()
        Thread { runCatching { CallApi.decide(m, req, behavior) }; pr.finish() }.start()
    }
}

class BootReceiver : BroadcastReceiver() {
    override fun onReceive(c: Context, i: Intent) { CallService.sync(c) }
}

class CallActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setShowWhenLocked(true); setTurnScreenOn(true)
        val x = intent
        val kind = x.getStringExtra("kind") ?: "done"
        val host = x.getStringExtra("host") ?: ""
        val req = x.getStringExtra("req") ?: ""
        val title = x.getStringExtra("title") ?: "Claude"
        getSystemService(NotificationManager::class.java).cancel(x.getIntExtra("nid", 0))
        val machine = Store.machines(this).firstOrNull { it.host == host }
        fun decide(b: String) {
            Thread { runCatching { machine?.let { CallApi.decide(it, req, b) } } }.start(); finish()
        }
        fun openChat(voice: Boolean = false) {
            startActivity(Intent(this, MainActivity::class.java).putExtra("open_host", host).putExtra("open_session", x.getStringExtra("session"))
                .putExtra("open_title", title).putExtra("open_cwd", x.getStringExtra("cwd")).putExtra("open_voice", voice).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TOP))
            finish()
        }
        setContent {
            ClaudeTheme {
                Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).systemBarsPadding().padding(32.dp),
                    verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
                    Spark(64.dp)
                    Spacer(Modifier.height(24.dp))
                    Label(when (kind) { "permission" -> "Needs permission"; "question" -> "Has a question"; else -> "Finished" })
                    Spacer(Modifier.height(8.dp))
                    Txt(title, T.title)
                    Spacer(Modifier.height(10.dp))
                    Txt(x.getStringExtra("text").orEmpty(), T.body, muted = true)
                    Spacer(Modifier.height(36.dp))
                    if (kind == "permission" && req.isNotEmpty()) {
                        Button("Allow", Modifier.fillMaxWidth()) { decide("allow") }
                        Spacer(Modifier.height(10.dp))
                        TextAction("Deny") { decide("deny") }
                        Spacer(Modifier.height(10.dp))
                    }
                    if (kind != "permission" || req.isEmpty()) {
                        Button("Talk", Modifier.fillMaxWidth()) { openChat(voice = true) }
                        Spacer(Modifier.height(10.dp))
                        TextAction("Open chat") { openChat() }
                    } else TextAction("Open chat") { openChat() }
                    Spacer(Modifier.height(10.dp))
                    TextAction("Dismiss") { finish() }
                }
            }
        }
    }
}
