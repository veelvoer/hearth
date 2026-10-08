package dev.voer.hearth

import android.content.Context
import android.content.SharedPreferences
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

private object Crypto {
    private const val ALIAS = "hearth.tokens"
    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val spec = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build()
        return KeyGenerator.getInstance("AES", "AndroidKeyStore").apply { init(spec) }.generateKey()
    }

    fun enc(s: String): String {
        val c = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val ct = c.doFinal(s.toByteArray())
        return Base64.encodeToString(c.iv, Base64.NO_WRAP) + ":" + Base64.encodeToString(ct, Base64.NO_WRAP)
    }

    fun dec(s: String): String {
        val (iv, ct) = s.split(":")
        val c = Cipher.getInstance("AES/GCM/NoPadding")
            .apply { init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, Base64.decode(iv, Base64.NO_WRAP))) }
        return String(c.doFinal(Base64.decode(ct, Base64.NO_WRAP)))
    }
}

data class Tokens(val access: String, val refresh: String, val expiresAt: Long)
data class Sample(val t: Long, val s: Double, val w: Double)

object Store {
    private fun sp(c: Context): SharedPreferences = c.applicationContext.getSharedPreferences("cm", 0)

    /** "Set up" now means: at least one computer or server is connected. */
    /** Older versions stored a Claude sign-in; this app no longer handles one, so forget it. */
    fun purgeOldLogin(c: Context) = sp(c).edit().remove("tok").remove("pv").remove("ps").remove("p_email").remove("p_plan").apply()

    fun signedIn(c: Context) = machines(c).isNotEmpty()
    /** The first-run tutorial: people who already connected a computer before it existed don't see it. */
    fun onboarded(c: Context) = sp(c).getBoolean("onboarded", machines(c).isNotEmpty())
    fun setOnboarded(c: Context, v: Boolean) = sp(c).edit().putBoolean("onboarded", v).apply()

    fun machines(c: Context): List<Machine> = runCatching {
        val a = JSONArray(Crypto.dec(sp(c).getString("machines", null)!!))
        List(a.length()) { val o = a.getJSONObject(it); Machine(o.getString("n"), o.getString("h"), o.getInt("p"), o.getString("t"), o.optBoolean("s")) }
    }.getOrDefault(emptyList())

    fun saveMachines(c: Context, l: List<Machine>) {
        val a = JSONArray()
        l.forEach { a.put(JSONObject().put("n", it.name).put("h", it.host).put("p", it.port).put("t", it.token).put("s", it.secure)) }
        sp(c).edit().putString("machines", Crypto.enc(a.toString())).apply()
    }

    fun usage(c: Context): Usage? = runCatching { Usage.from(JSONObject(sp(c).getString("usage", null)!!)) }.getOrNull()
    fun saveUsage(c: Context, u: Usage) = sp(c).edit().putString("usage", u.toJson().toString()).apply()

    fun profile(c: Context): Profile? = sp(c).let {
        val e = it.getString("p_email", null) ?: return null
        Profile(e, it.getString("p_plan", "") ?: "")
    }

    fun saveProfile(c: Context, p: Profile?) = sp(c).edit().apply {
        if (p == null) { remove("p_email"); remove("p_plan") } else { putString("p_email", p.email); putString("p_plan", p.plan) }
    }.apply()

    var Context.error: String?
        get() = sp(this).getString("err", null)
        set(v) { sp(this).edit().putString("err", v).apply() }

    // settings
    fun thresholds(c: Context): Set<Int> =
        (sp(c).getString("thr", "75,90,100") ?: "").split(",").mapNotNull { it.toIntOrNull() }.toSet()
    fun setThresholds(c: Context, s: Set<Int>) = sp(c).edit().putString("thr", s.sorted().joinToString(",")).apply()
    fun interval(c: Context) = sp(c).getInt("interval", 15)
    fun setInterval(c: Context, m: Int) = sp(c).edit().putInt("interval", m).apply()
    fun notifyReset(c: Context) = sp(c).getBoolean("nreset", true)
    fun setNotifyReset(c: Context, v: Boolean) = sp(c).edit().putBoolean("nreset", v).apply()
    fun callMe(c: Context) = sp(c).getBoolean("callme", false)
    fun setCallMe(c: Context, v: Boolean) = sp(c).edit().putBoolean("callme", v).apply()
    fun supportToken(c: Context): String? = sp(c).getString("support_t", null)?.let { runCatching { Crypto.dec(it) }.getOrNull() }
    fun supportEmail(c: Context): String? = sp(c).getString("support_e", null)
    fun setSupport(c: Context, token: String?, email: String?) = sp(c).edit().apply { if (token == null) { remove("support_t"); remove("support_e") } else { putString("support_t", Crypto.enc(token)); putString("support_e", email) } }.apply()
    fun lastMachine(c: Context): String? = sp(c).getString("lastm", null)
    fun setLastMachine(c: Context, host: String) = sp(c).edit().putString("lastm", host).apply()
    /** Language the call is held in: "en" or "nl". Replies are still spoken in the language they are written in. */
    fun voiceLang(c: Context): String = sp(c).getString("vlang", null) ?: if (java.util.Locale.getDefault().language == "nl") "nl" else "en"
    fun setVoiceLang(c: Context, v: String) = sp(c).edit().putString("vlang", v).apply()
    fun chatModel(c: Context): String = sp(c).getString("cmodel", "") ?: ""
    fun setChatModel(c: Context, v: String) = sp(c).edit().putString("cmodel", v).apply()
    fun chatEffort(c: Context): String = sp(c).getString("ceffort", "") ?: ""
    fun setChatEffort(c: Context, v: String) = sp(c).edit().putString("ceffort", v).apply()
    fun themeSystem(c: Context) = sp(c).getBoolean("themesys", false)
    fun setThemeSystem(c: Context, v: Boolean) = sp(c).edit().putBoolean("themesys", v).apply()
    fun voiceBrief(c: Context) = sp(c).getBoolean("vbrief", false)
    fun setVoiceBrief(c: Context, v: Boolean) = sp(c).edit().putBoolean("vbrief", v).apply()
    fun voiceEngine(c: Context) = sp(c).getString("voice", "auto") ?: "auto"
    fun setVoiceEngine(c: Context, v: String) = sp(c).edit().putString("voice", v).apply()
    fun live(c: Context) = sp(c).getBoolean("live", false)
    fun setLive(c: Context, v: Boolean) = sp(c).edit().putBoolean("live", v).apply()
    fun lastNotified(c: Context, k: String): String? = sp(c).getString("ln_$k", null)
    fun setLastNotified(c: Context, k: String, v: String) = sp(c).edit().putString("ln_$k", v).apply()

    // widget configs
    fun widget(c: Context, id: Int): WCfg =
        (sp(c).getString("w$id", null) ?: sp(c).getString("w0", null))?.let { WCfg.from(it) } ?: WCfg()
    fun saveWidget(c: Context, id: Int, w: WCfg) = sp(c).edit().putString("w$id", w.toJson()).apply()
    fun dropWidget(c: Context, id: Int) = sp(c).edit().remove("w$id").apply()

    // history
    private fun hf(c: Context) = File(c.applicationContext.filesDir, "history.json")

    @Synchronized
    fun history(c: Context): List<Sample> = runCatching {
        val a = JSONArray(hf(c).readText())
        List(a.length()) { val r = a.getJSONArray(it); Sample(r.getLong(0), r.getDouble(1), r.getDouble(2)) }
    }.getOrDefault(emptyList())

    @Synchronized
    fun appendHistory(c: Context, u: Usage) {
        val cutoff = u.at - 30L * 24 * 3600_000
        val list = history(c).filter { it.t >= cutoff }.toMutableList()
        val s = u.session?.at(u.at) ?: 0.0
        val w = u.week?.at(u.at) ?: 0.0
        val last = list.lastOrNull()
        if (last == null || u.at - last.t > 4 * 60_000 || last.s != s || last.w != w) list += Sample(u.at, s, w)
        val a = JSONArray()
        list.forEach { a.put(JSONArray().put(it.t).put(it.s).put(it.w)) }
        hf(c).writeText(a.toString())
    }
}
