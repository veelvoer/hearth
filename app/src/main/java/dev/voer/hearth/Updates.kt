package dev.voer.hearth

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.provider.Settings
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.launch
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL

/** Updates from GitHub, without a cable: look at the newest release, download the phone app, let Android install it. */
object Updates {
    data class Release(val tag: String, val version: String, val notes: String, val apkUrl: String?)

    var release by mutableStateOf<Release?>(null)     // set when a newer version than this one exists
    var busy by mutableStateOf(false)
    var progress by mutableStateOf(-1)                // 0..100 while downloading
    var message by mutableStateOf("")
    var checkedAt by mutableStateOf(0L)
    var latestVersion by mutableStateOf("")

    fun current(c: Context): String = runCatching { c.packageManager.getPackageInfo(c.packageName, 0).versionName ?: "0" }.getOrDefault("0")
    private fun parts(v: String) = v.removePrefix("v").split('.').map { it.toIntOrNull() ?: 0 }
    fun newer(a: String, b: String): Boolean { val x = parts(a); val y = parts(b); for (i in 0..2) { val p = x.getOrElse(i) { 0 }; val q = y.getOrElse(i) { 0 }; if (p != q) return p > q }; return false }

    suspend fun latest(): Release = withContext(Dispatchers.IO) {
        val c = URL("https://api.github.com/repos/veelvoer/hearth/releases/latest").openConnection() as HttpURLConnection
        c.connectTimeout = 8_000; c.readTimeout = 12_000; c.setRequestProperty("Accept", "application/vnd.github+json"); c.setRequestProperty("User-Agent", "Hearth-app")
        try {
            if (c.responseCode != 200) throw ApiException(c.responseCode, "GitHub said ${c.responseCode}")
            val o = JSONObject(c.inputStream.bufferedReader().readText())
            val assets = o.optJSONArray("assets")
            var apk: String? = null
            if (assets != null) for (i in 0 until assets.length()) { val a = assets.getJSONObject(i); if (a.getString("name").endsWith(".apk")) apk = a.getString("browser_download_url") }
            Release(o.getString("tag_name"), o.getString("tag_name").removePrefix("v"), o.optString("body"), apk)
        } finally { c.disconnect() }
    }

    /** Looks on GitHub. Sets [release] when something newer is there. Returns an error text or null. */
    suspend fun check(c: Context): String? {
        busy = true; message = ""
        return try {
            val r = latest(); checkedAt = System.currentTimeMillis(); latestVersion = r.version
            release = if (newer(r.version, current(c))) r else null
            null
        } catch (e: Exception) { "Could not look for updates: ${e.message ?: "no connection"}" } finally { busy = false }
    }

    /** Downloads the new phone app and hands it to Android's installer (one tap on "Update"). */
    suspend fun install(c: Context, r: Release) {
        val url = r.apkUrl ?: run { message = "The new version has no phone app file."; return }
        if (!c.packageManager.canRequestPackageInstalls()) {
            message = "Allow Hearth to install updates, then press Update again."
            c.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${c.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)); return
        }
        busy = true; progress = 0; message = "Downloading…"
        try {
            val file = withContext(Dispatchers.IO) {
                val f = File(c.cacheDir, "hearth-update.apk")
                val con = URL(url).openConnection() as HttpURLConnection
                con.connectTimeout = 10_000; con.readTimeout = 30_000; con.setRequestProperty("User-Agent", "Hearth-app")
                try {
                    val total = con.contentLengthLong; var got = 0L
                    con.inputStream.use { i -> f.outputStream().use { o -> val buf = ByteArray(64 * 1024); while (true) { val n = i.read(buf); if (n < 0) break; o.write(buf, 0, n); got += n; if (total > 0) progress = (got * 100 / total).toInt() } } }
                } finally { con.disconnect() }
                f
            }
            message = "Installing…"
            withContext(Dispatchers.IO) {
                val pi = c.packageManager.packageInstaller
                val id = pi.createSession(PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL))
                pi.openSession(id).use { s ->
                    file.inputStream().use { i -> s.openWrite("hearth.apk", 0, file.length()).use { o -> i.copyTo(o); s.fsync(o) } }
                    val pending = PendingIntent.getBroadcast(c, id, Intent(c, InstallReceiver::class.java).setPackage(c.packageName), PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
                    s.commit(pending.intentSender)
                }
            }
        } catch (e: Exception) { message = "The update did not work: ${e.message}" } finally { busy = false; progress = -1 }
    }

    // ── servers ──
    data class ServerInfo(val version: String?, val canUpdate: Boolean)
    suspend fun serverInfo(m: Machine): ServerInfo? = withContext(Dispatchers.IO) {
        runCatching {
            val c = Relay.open(m, "/status")
            try { if (c.responseCode != 200) null else JSONObject(c.inputStream.bufferedReader().readText()).let { ServerInfo(it.optString("version").ifBlank { null }, it.optBoolean("canSelfUpdate")) } } finally { c.disconnect() }
        }.getOrNull()
    }
    /** Asks the server to download the newest version and restart. Returns null when it worked, or a text. */
    suspend fun updateServer(m: Machine): String? = withContext(Dispatchers.IO) {
        runCatching {
            val c = Relay.open(m, "/admin/update", post = true, read = 120_000)
            try {
                c.outputStream.use { it.write("{}".toByteArray()) }
                val body = (if (c.responseCode == 200) c.inputStream else c.errorStream).bufferedReader().readText()
                if (c.responseCode == 200) null else JSONObject(body).optString("error", "The server said ${c.responseCode}")
            } finally { c.disconnect() }
        }.getOrElse { it.message ?: "Could not reach the server" }
    }
}

/** Android tells us how the install went. It may ask the person to confirm first. */
class InstallReceiver : BroadcastReceiver() {
    override fun onReceive(c: Context, i: Intent) {
        when (i.getIntExtra(PackageInstaller.EXTRA_STATUS, -1)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                @Suppress("DEPRECATION") val confirm = i.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)
                confirm?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)?.let { c.startActivity(it) }
            }
            PackageInstaller.STATUS_SUCCESS -> Updates.message = "Updated"
            else -> Updates.message = i.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE)?.let { "The update did not install: $it" } ?: "The update did not install."
        }
    }
}

/** Settings → Updates: this phone app and every server it is connected to. */
@Composable
fun UpdatesCard() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    val servers = remember(Ui.machinesRev) { Store.machines(c).filter { it.secure } }
    var infos by remember { mutableStateOf<Map<String, Updates.ServerInfo?>>(emptyMap()) }
    var note by remember { mutableStateOf("") }
    var updating by remember { mutableStateOf("") }
    suspend fun look() { note = Updates.check(c) ?: ""; infos = servers.associate { it.host to Updates.serverInfo(it) } }
    LaunchedEffect(servers.size) { look() }
    val r = Updates.release
    Card {
        Label("Updates")
        Spacer(Modifier.height(6.dp))
        Txt("Hearth ${Updates.current(c)}", T.body)
        Txt(when {
            Updates.progress >= 0 -> "Downloading… ${Updates.progress}%"
            Updates.busy -> "Looking…"
            r != null -> "Version ${r.version} is ready"
            Updates.checkedAt > 0 -> "This is the newest version"
            else -> ""
        }, T.small, muted = true)
        listOf(note, Updates.message).filter { it.isNotBlank() }.forEach { Txt(it, T.small, color = androidx.compose.material3.MaterialTheme.colorScheme.primary) }
        Spacer(Modifier.height(8.dp))
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
            TextAction(if (Updates.busy) "Looking…" else "Check for updates") { if (!Updates.busy) scope.launch { look() } }
            if (r != null) Button("Update", Modifier) { if (!Updates.busy) scope.launch { Updates.install(c, r) } }
        }
        servers.forEach { m ->
            val info = infos[m.host]
            Spacer(Modifier.height(12.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Txt("Server · ${m.name}", T.body)
                    Txt(when {
                        !infos.containsKey(m.host) -> "Looking…"
                        info == null -> "Can't be reached right now"
                        info.version == null -> "Older server: run the installer line again to update it"
                        Updates.latestVersion.isNotEmpty() && Updates.newer(Updates.latestVersion, info.version) -> "Version ${info.version} · update to ${Updates.latestVersion}"
                        else -> "Version ${info.version} · up to date"
                    }, T.small, muted = true)
                }
                if (info?.version != null && info.canUpdate && Updates.latestVersion.isNotEmpty() && Updates.newer(Updates.latestVersion, info.version))
                    Button(if (updating == m.host) "Updating…" else "Update", Modifier) {
                        if (updating.isEmpty()) scope.launch { updating = m.host; note = Updates.updateServer(m) ?: "Server updated"; kotlinx.coroutines.delay(4000); infos = infos + (m.host to Updates.serverInfo(m)); updating = "" }
                    }
            }
        }
    }
}
