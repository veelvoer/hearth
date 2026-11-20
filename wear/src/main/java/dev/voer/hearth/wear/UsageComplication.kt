package dev.voer.hearth.wear

import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import androidx.compose.ui.graphics.toArgb
import androidx.wear.watchface.complications.data.ColorRamp
import androidx.wear.watchface.complications.data.ComplicationData
import androidx.wear.watchface.complications.data.ComplicationType
import androidx.wear.watchface.complications.data.LongTextComplicationData
import androidx.wear.watchface.complications.data.PlainComplicationText
import androidx.wear.watchface.complications.data.RangedValueComplicationData
import androidx.wear.watchface.complications.data.ShortTextComplicationData
import androidx.wear.watchface.complications.datasource.ComplicationDataSourceUpdateRequester
import androidx.wear.watchface.complications.datasource.ComplicationRequest
import androidx.wear.watchface.complications.datasource.SuspendingComplicationDataSourceService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** What one complication shows: a value 0..100 (for a ring), a short text, a title, and a longer line. */
class Shown(val value: Float?, val text: String, val title: String, val long: String = text)

/**
 * What Hearth puts on a watch face. Every value carries the colours of the theme chosen in the watch app (as its colour ramp),
 * so the Hearth watch face, which draws with them, follows the theme. A tap opens Hearth.
 * Battery: all complications that update together share one fetch (one Bluetooth message to the phone), at most every few minutes.
 */
abstract class HearthComplication : SuspendingComplicationDataSourceService() {
    protected abstract val name: String
    protected abstract fun shape(h: Home?): Shown
    /** The colours handed to the face: the accent only (any face can draw a ring with it), or the whole theme (the Hearth face). */
    protected open fun colors(h: Home?): IntArray { val a = accent(h); return intArrayOf(a, a) }

    private fun tap() = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)

    override fun getPreviewData(type: ComplicationType): ComplicationData? { Pal.load(applicationContext); return build(type, Shown(42f, "42%", "5h", "Claude: all quiet"), colors(null)) }

    override suspend fun onComplicationRequest(request: ComplicationRequest): ComplicationData? {
        Phone.app = applicationContext
        Pal.load(applicationContext)
        val h = withContext(Dispatchers.IO) { Servers.all(this@HearthComplication).firstNotNullOfOrNull { s -> runCatching { Api.homeRecent(s) }.getOrNull() ?: Api.cached(s) } }
        return build(request.complicationType, shape(h), colors(h))
    }

    protected fun accent(h: Home?): Int = if ((h?.usage?.session ?: 0.0) >= 90) 0xFFE5484D.toInt() else Pal.cur.clay.toArgb()

    private fun build(type: ComplicationType, s: Shown, ramp: IntArray): ComplicationData? {
        val txt = PlainComplicationText.Builder(s.text).build()
        val title = PlainComplicationText.Builder(s.title).build()
        val desc = PlainComplicationText.Builder(name).build()
        return when (type) {
            ComplicationType.RANGED_VALUE -> RangedValueComplicationData.Builder((s.value ?: 0f).coerceIn(0f, 100f), 0f, 100f, desc).setText(txt).setTitle(title).setColorRamp(ColorRamp(ramp, false)).setTapAction(tap()).build()
            ComplicationType.SHORT_TEXT -> ShortTextComplicationData.Builder(txt, desc).setTitle(title).setTapAction(tap()).build()
            ComplicationType.LONG_TEXT -> LongTextComplicationData.Builder(PlainComplicationText.Builder(s.long).build(), desc).setTitle(title).setTapAction(tap()).build()
            else -> null
        }
    }
}

private fun pctText(v: Double?) = v?.let { "${it.toInt()}%" } ?: "–"

/** One line about what Claude does: waiting for you, working (and on which chat), or quiet. */
private fun status(h: Home?): Pair<String, String> {
    if (h == null) return "–" to "Hearth: no connection"
    val asks = h.asks.size; val busy = h.chats.filter { it.busy }
    return when {
        asks > 0 -> "$asks" to (if (asks == 1) "Claude needs you" else "$asks need you")
        busy.isNotEmpty() -> "${busy.size}" to (if (busy.size == 1) "Working: ${busy[0].title.take(40)}" else "${busy.size} chats working")
        else -> "0" to "All quiet"
    }
}

/** The 5-hour usage: a ring or "72%". */
class UsageComplication : HearthComplication() {
    override val name = "Claude usage"
    override fun shape(h: Home?) = h?.usage?.session.let { Shown(it?.toFloat(), pctText(it), "5h", "5h usage ${pctText(it)}") }
}

/** The week's usage. */
class WeekComplication : HearthComplication() {
    override val name = "Claude week"
    override fun shape(h: Home?) = h?.usage?.week.let { Shown(it?.toFloat(), pctText(it), "week", "Week usage ${pctText(it)}") }
}

/** How many chats work now, or how many wait for you; the long form names the chat. */
class BusyComplication : HearthComplication() {
    override val name = "Claude chats"
    override fun shape(h: Home?): Shown { val (n, line) = status(h); return Shown(null, n, if ((h?.asks?.size ?: 0) > 0) "waiting" else "working", line) }
}

/**
 * The Hearth watch face's own source: the 5-hour usage for the ring, the status line, and the theme as four colours
 * (accent, text, quiet text, ring track) that the face picks with extractColorFromColors at 0.05 / 0.4 / 0.7 / 1.0.
 */
class FaceComplication : HearthComplication() {
    override val name = "Hearth face"
    override fun shape(h: Home?): Shown { val u = h?.usage?.session; val (_, line) = status(h); return Shown(u?.toFloat(), pctText(u), line, line) }
    override fun colors(h: Home?): IntArray = intArrayOf(accent(h), Pal.cur.ink.toArgb(), Pal.cur.muted.toArgb(), Pal.cur.card.toArgb())
}

object Complications {
    private val ALL = listOf(FaceComplication::class.java, UsageComplication::class.java, WeekComplication::class.java, BusyComplication::class.java)
    /** The theme or the numbers changed: every Hearth complication on the face is asked again (the face redraws once). */
    fun refresh(c: Context) {
        for (k in ALL) runCatching { ComplicationDataSourceUpdateRequester.create(c, ComponentName(c, k)).requestUpdateAll() }
    }
}
