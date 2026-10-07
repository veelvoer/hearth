package dev.voer.hearth

import android.Manifest
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.service.quicksettings.TileService
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.work.*
import dev.voer.hearth.Store.error
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import java.util.concurrent.TimeUnit

object Repo {
    private val lock = Mutex()

    /** Usage comes from your connected computers (the real Claude Code reports it). [ask] = you pressed Refresh: read it again first. */
    suspend fun refresh(c: Context, ask: Boolean = false): Result<Usage> = lock.withLock {
        withContext(Dispatchers.IO) {
            val app = c.applicationContext
            runCatching {
                val machines = Store.machines(app)
                if (machines.isEmpty()) throw ApiException(0, "No computer connected")
                if (ask) machines.firstOrNull { !it.secure }?.let { runCatching { Relay.refreshUsage(it) } } ?: runCatching { Relay.refreshUsage(machines.first()) }
                val u = machines.mapNotNull { runCatching { Relay.usage(it) }.getOrNull() }.maxByOrNull { it.at }
                    ?: throw ApiException(0, "No usage yet")
                val prev = Store.usage(app)
                Store.saveUsage(app, u)
                if (prev == null || prev.at != u.at) { Store.appendHistory(app, u); Notifier.check(app, prev, u) }
                app.error = null
                WidgetUpdater.updateAll(app)
                TileService.requestListeningState(app, android.content.ComponentName(app, UsageTile::class.java))
                u
            }.onFailure {
                app.error = if (it is ApiException && it.message == "No usage yet") "No usage data yet. Start a Claude Code chat from here, or tap Refresh."
                    else "Can't reach your computer right now."
                WidgetUpdater.updateAll(app)
            }
        }
    }
}

object Notifier {
    private const val ALERTS = "alerts"
    private const val LIVE = "live"

    private fun channels(c: Context) {
        val nm = c.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(ALERTS, "Limit alerts", NotificationManager.IMPORTANCE_DEFAULT))
        nm.createNotificationChannel(NotificationChannel(LIVE, "Live usage", NotificationManager.IMPORTANCE_LOW))
    }

    private fun open(c: Context) = PendingIntent.getActivity(c, 0, Intent(c, MainActivity::class.java),
        PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)

    private fun post(c: Context, id: Int, n: android.app.Notification) {
        if (ContextCompat.checkSelfPermission(c, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) return
        runCatching { NotificationManagerCompat.from(c).notify(id, n) }
    }

    fun check(c: Context, prev: Usage?, now: Usage) {
        channels(c)
        val thr = Store.thresholds(c).sortedDescending()
        listOf(Triple("session", now.session, "Session (5h)"), Triple("week", now.week, "Weekly")).forEachIndexed { i, (k, l, name) ->
            l ?: return@forEachIndexed
            val win = ((l.resetsAt ?: 0) / 3600_000).toString()
            val last = Store.lastNotified(c, k)?.split(":")
            val done = if (last?.getOrNull(0) == win) last[1].toIntOrNull() ?: 0 else 0
            val hit = thr.firstOrNull { l.pct >= it && it > done } ?: return@forEachIndexed
            Store.setLastNotified(c, k, "$win:$hit")
            val left = l.resetsAt?.let { " Resets in ${dur(it - now.at)}." }.orEmpty()
            val title = if (hit >= 100) "$name limit reached" else "$name at ${l.pct.toInt()}%"
            post(c, 10 + i, NotificationCompat.Builder(c, ALERTS).setSmallIcon(R.drawable.ic_stat)
                .setContentTitle(title).setContentText(left.trim().ifEmpty { "Crossed $hit%." })
                .setContentIntent(open(c)).setAutoCancel(true).build())
        }
        val ps = prev?.session; val ns = now.session
        if (Store.notifyReset(c) && ps != null && ns != null && ps.pct >= 75 && ns.pct < ps.pct - 30)
            post(c, 20, NotificationCompat.Builder(c, ALERTS).setSmallIcon(R.drawable.ic_stat)
                .setContentTitle("Session reset").setContentText("A fresh 5-hour window is ready.")
                .setContentIntent(open(c)).setAutoCancel(true).build())
        live(c, now)
    }

    fun live(c: Context, u: Usage?) {
        val nm = NotificationManagerCompat.from(c)
        if (!Store.live(c) || u == null) { nm.cancel(30); return }
        channels(c)
        val s = u.session; val w = u.week
        val sp = s?.at(u.at) ?: 0.0
        val title = "Session ${sp.toInt()}%" + (s?.resetsAt?.takeIf { it > u.at }?.let { " · resets in ${dur(it - u.at)}" } ?: "")
        post(c, 30, NotificationCompat.Builder(c, LIVE).setSmallIcon(R.drawable.ic_stat)
            .setContentTitle(title).setContentText("Week ${(w?.at(u.at) ?: 0.0).toInt()}%")
            .setProgress(100, sp.toInt(), false).setOngoing(true).setOnlyAlertOnce(true).setSilent(true)
            .setContentIntent(open(c)).build())
    }
}

class UsageWorker(c: Context, p: WorkerParameters) : CoroutineWorker(c, p) {
    override suspend fun doWork(): Result {
        if (Store.signedIn(applicationContext)) Repo.refresh(applicationContext)
        CallService.sync(applicationContext)  // watchdog: bring the background service back if the system stopped it
        return Result.success()
    }
}

object Sched {
    private val net = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()

    fun ensure(c: Context, replace: Boolean = false) {
        val req = PeriodicWorkRequestBuilder<UsageWorker>(Store.interval(c).toLong(), TimeUnit.MINUTES).setConstraints(net).build()
        WorkManager.getInstance(c).enqueueUniquePeriodicWork("usage", if (replace) ExistingPeriodicWorkPolicy.UPDATE else ExistingPeriodicWorkPolicy.KEEP, req)
    }

    fun now(c: Context) {
        WorkManager.getInstance(c).enqueueUniqueWork("usage-now", ExistingWorkPolicy.KEEP,
            OneTimeWorkRequestBuilder<UsageWorker>().setConstraints(net).build())
    }
}
