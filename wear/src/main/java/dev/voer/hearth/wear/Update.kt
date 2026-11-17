package dev.voer.hearth.wear

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.net.Uri
import android.provider.Settings
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.zip.ZipInputStream

/** Updating the watch app from the watch: looks on GitHub, downloads the watch app of the newest release and hands it to Android's installer. */
object Update {
    var status by mutableStateOf("")
    var busy by mutableStateOf(false)
    var latest by mutableStateOf("")
    var url: String? = null

    fun current(c: Context): String = c.packageManager.getPackageInfo(c.packageName, 0).versionName ?: ""
    private fun parse(v: String) = v.removePrefix("v").split('.').map { it.toIntOrNull() ?: 0 }
    fun newer(a: String, b: String): Boolean { val x = parse(a); val y = parse(b); for (i in 0 until 3) { val p = x.getOrElse(i) { 0 }; val q = y.getOrElse(i) { 0 }; if (p != q) return p > q }; return false }

    /** The newest release's watch file: the .apk, or the .zip with the .apk inside (the phone app's own update ignores a watch .apk, so it goes out zipped). */
    fun pick(release: JSONObject): Pair<String, String?> {
        val tag = release.optString("tag_name").removePrefix("v"); var found: String? = null
        val a = release.optJSONArray("assets")
        if (a != null) for (i in 0 until a.length()) { val o = a.getJSONObject(i); val n = o.getString("name"); if (n.startsWith("Hearth-watch-") && (n.endsWith(".zip") || n.endsWith(".apk"))) found = o.getString("browser_download_url") }
        return tag to found
    }

    suspend fun check(c: Context) {
        busy = true; status = "Looking…"
        try {
            val (tag, u) = withContext(Dispatchers.IO) {
                val con = URL("https://api.github.com/repos/veelvoer/hearth/releases/latest").openConnection() as HttpURLConnection
                con.connectTimeout = 8_000; con.readTimeout = 12_000; con.setRequestProperty("Accept", "application/vnd.github+json"); con.setRequestProperty("User-Agent", "Hearth-watch")
                try { if (con.responseCode != 200) error("GitHub said ${con.responseCode}"); pick(JSONObject(con.inputStream.bufferedReader().readText())) } finally { con.disconnect() }
            }
            latest = tag; url = u
            status = when { u == null -> "No watch file in the newest version"; newer(tag, current(c)) -> "Version $tag is ready"; else -> "This is the newest version" }
        } catch (e: Exception) { status = "Can't look: ${e.message ?: "no connection"}" } finally { busy = false }
    }

    fun ready(c: Context) = url != null && newer(latest, current(c))

    suspend fun install(c: Context) {
        val u = url ?: return
        if (!c.packageManager.canRequestPackageInstalls()) {
            status = "Allow Hearth to install, then press Update again"
            c.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${c.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)); return
        }
        busy = true; status = "Downloading…"
        try {
            val apk = withContext(Dispatchers.IO) {
                val f = File(c.cacheDir, "hearth-update.apk")
                val con = URL(u).openConnection() as HttpURLConnection
                con.connectTimeout = 10_000; con.readTimeout = 30_000; con.setRequestProperty("User-Agent", "Hearth-watch")
                try {
                    if (u.endsWith(".zip")) ZipInputStream(con.inputStream).use { z -> var e = z.nextEntry; while (e != null && !e.name.endsWith(".apk")) e = z.nextEntry; if (e == null) error("no app in the file"); f.outputStream().use { z.copyTo(it) } }
                    else con.inputStream.use { i -> f.outputStream().use { i.copyTo(it) } }
                } finally { con.disconnect() }
                f
            }
            status = "Installing…"
            withContext(Dispatchers.IO) {
                val pi = c.packageManager.packageInstaller
                val id = pi.createSession(PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL))
                pi.openSession(id).use { s ->
                    apk.inputStream().use { i -> s.openWrite("hearth.apk", 0, apk.length()).use { o -> i.copyTo(o); s.fsync(o) } }
                    s.commit(PendingIntent.getBroadcast(c, id, Intent(c, InstallReceiver::class.java).setPackage(c.packageName), PendingIntent.FLAG_MUTABLE or PendingIntent.FLAG_UPDATE_CURRENT).intentSender)
                }
            }
        } catch (e: Exception) { status = "The update did not work: ${e.message}" } finally { busy = false }
    }

    fun cleanup(c: Context) { runCatching { File(c.cacheDir, "hearth-update.apk").delete() } }
}

class InstallReceiver : BroadcastReceiver() {
    override fun onReceive(c: Context, i: Intent) {
        when (i.getIntExtra(PackageInstaller.EXTRA_STATUS, -1)) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> { @Suppress("DEPRECATION") val confirm = i.getParcelableExtra<Intent>(Intent.EXTRA_INTENT); confirm?.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)?.let { c.startActivity(it) } }
            PackageInstaller.STATUS_SUCCESS -> Update.status = "Updated"
            else -> Update.status = "The update did not install"
        }
    }
}
