package dev.voer.hearth.wear

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.graphics.Color
import org.json.JSONObject

/** The watch's colors. One choice: the watch's own look, the look of your computer, or a preset. Every screen reads these. */
object Pal {
    class Set(val name: String, val clay: Color, val bg: Color, val card: Color, val ink: Color, val muted: Color, val onClay: Color = Color(0xFF1B1A19))
    val PRESETS = listOf(
        Set("Hearth clay", Color(0xFFF0643C), Color.Black, Color(0xFF252422), Color(0xFFF4F2EC), Color(0xFF9A968C)),
        Set("Ocean blue", Color(0xFF5AA9F5), Color.Black, Color(0xFF1B2733), Color(0xFFEAF2FA), Color(0xFF8FA3B5)),
        Set("Forest green", Color(0xFF6FCF7F), Color.Black, Color(0xFF1C2A1F), Color(0xFFEAF5EC), Color(0xFF8DA591)),
        Set("Grape", Color(0xFFB98CF0), Color.Black, Color(0xFF282033), Color(0xFFF1EAFA), Color(0xFFA195B3)),
        Set("Mono", Color(0xFFE6E6E6), Color.Black, Color(0xFF1F1F1F), Color(0xFFFFFFFF), Color(0xFF8C8C8C)),
    )
    private val WATCH = PRESETS[0]

    var choice by mutableStateOf("watch")      // "watch", "pc" or a preset name
    var cur by mutableStateOf(WATCH)
    var pcName by mutableStateOf("")           // the computer's theme, when we have it

    private fun sp(c: Context) = c.getSharedPreferences("look", Context.MODE_PRIVATE)
    private fun hex(o: JSONObject, k: String): Color? = o.optString(k).takeIf { it.length >= 7 && it.startsWith("#") }?.let { runCatching { Color(android.graphics.Color.parseColor(it.take(7))) }.getOrNull() }

    /** The computer's theme as colors, or null. Light themes are turned into the dark look a watch needs: only its accent is kept. */
    private fun fromPc(o: JSONObject): Set? {
        val accent = hex(o, "accent") ?: return null
        if (o.optBoolean("light")) return Set(o.optString("name").ifBlank { "My computer" }, accent, Color.Black, Color(0xFF252422), Color(0xFFF4F2EC), Color(0xFF9A968C))
        val bg = hex(o, "bg") ?: return null
        return Set(o.optString("name").ifBlank { "My computer" }, accent, Color.Black, hex(o, "surface") ?: bg, hex(o, "text") ?: Color(0xFFF4F2EC), hex(o, "muted") ?: Color(0xFF9A968C), hex(o, "ink") ?: Color(0xFF1B1A19))
    }

    fun load(c: Context) {
        choice = sp(c).getString("choice", "watch") ?: "watch"
        pcName = runCatching { fromPc(JSONObject(sp(c).getString("pc", "{}") ?: "{}"))?.name ?: "" }.getOrDefault("")
        apply(c)
    }
    fun pick(c: Context, v: String) { choice = v; sp(c).edit().putString("choice", v).apply(); apply(c); Complications.refresh(c) }
    /** Remembers what the computer said (null: it has no theme to share). */
    fun savePc(c: Context, o: JSONObject?) { sp(c).edit().putString("pc", o?.toString() ?: "{}").apply(); pcName = o?.let { fromPc(it)?.name } ?: ""; val before = cur; apply(c); if (choice == "pc" && cur != before) Complications.refresh(c) }
    private fun apply(c: Context) {
        cur = when (choice) {
            "pc" -> runCatching { fromPc(JSONObject(sp(c).getString("pc", "{}") ?: "{}")) }.getOrNull() ?: WATCH
            "watch" -> WATCH
            else -> PRESETS.firstOrNull { it.name == choice } ?: WATCH
        }
    }
}
