package dev.voer.hearth.wear

import android.app.PendingIntent
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import androidx.compose.ui.graphics.toArgb
import androidx.wear.watchface.complications.data.ColorRamp
import androidx.wear.watchface.complications.data.ComplicationData
import androidx.wear.watchface.complications.data.ComplicationType
import androidx.wear.watchface.complications.data.PlainComplicationText
import androidx.wear.watchface.complications.data.RangedValueComplicationData
import androidx.wear.watchface.complications.data.ShortTextComplicationData
import androidx.wear.watchface.complications.datasource.ComplicationDataSourceUpdateRequester
import androidx.wear.watchface.complications.datasource.ComplicationRequest
import androidx.wear.watchface.complications.datasource.SuspendingComplicationDataSourceService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * What Hearth puts on a watch face. Every data carries the accent of the Hearth theme chosen on the watch as its colour ramp,
 * so a face that draws with it (the Hearth watch face does) follows the theme. A tap opens Hearth.
 */
abstract class HearthComplication : SuspendingComplicationDataSourceService() {
    protected abstract val name: String
    /** (value 0..100 or null, text, title) of this complication for the home numbers. */
    protected abstract fun shape(h: Home?, v: Double?): Triple<Float?, String, String>

    private fun tap() = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)

    override fun getPreviewData(type: ComplicationType): ComplicationData? = build(type, Triple(42f, "42%", name), accent())

    override suspend fun onComplicationRequest(request: ComplicationRequest): ComplicationData? {
        Phone.app = applicationContext
        Pal.load(applicationContext)
        val h = withContext(Dispatchers.IO) { Servers.all(this@HearthComplication).firstNotNullOfOrNull { s -> runCatching { Api.home(s) }.getOrNull() } }
        return build(request.complicationType, shape(h, h?.usage?.session), accent(h?.usage?.session))
    }

    private fun accent(v: Double? = null): Int = if ((v ?: 0.0) >= 90) 0xFFE5484D.toInt() else Pal.cur.clay.toArgb()

    private fun build(type: ComplicationType, s: Triple<Float?, String, String>, color: Int): ComplicationData? {
        val txt = PlainComplicationText.Builder(s.second).build()
        val title = PlainComplicationText.Builder(s.third).build()
        val desc = PlainComplicationText.Builder(name).build()
        return when (type) {
            ComplicationType.RANGED_VALUE -> RangedValueComplicationData.Builder((s.first ?: 0f).coerceIn(0f, 100f), 0f, 100f, desc).setText(txt).setTitle(title).setColorRamp(ColorRamp(intArrayOf(color, color), false)).setTapAction(tap()).build()
            ComplicationType.SHORT_TEXT -> ShortTextComplicationData.Builder(txt, desc).setTitle(title).setTapAction(tap()).build()
            else -> null
        }
    }
}

/** The 5-hour usage: a ring or "72%". */
class UsageComplication : HearthComplication() {
    override val name = "Claude usage"
    override fun shape(h: Home?, v: Double?) = Triple(v?.toFloat(), v?.let { "${it.toInt()}%" } ?: "–", "5h")
}

/** The week's usage. */
class WeekComplication : HearthComplication() {
    override val name = "Claude week"
    override fun shape(h: Home?, v: Double?): Triple<Float?, String, String> { val w = h?.usage?.week; return Triple(w?.toFloat(), w?.let { "${it.toInt()}%" } ?: "–", "week") }
}

/** How many chats work now, or how many wait for you. */
class BusyComplication : HearthComplication() {
    override val name = "Claude chats"
    override fun shape(h: Home?, v: Double?): Triple<Float?, String, String> {
        val asks = h?.asks?.size ?: 0; val busy = h?.chats?.count { it.busy } ?: 0
        return if (asks > 0) Triple(null, "$asks", "waiting") else Triple(null, "$busy", if (busy == 1) "working" else "working")
    }
}

object Complications {
    /** The theme changed: every Hearth complication on a face is asked for new colours at once. */
    fun refresh(c: Context) {
        for (k in listOf(UsageComplication::class.java, WeekComplication::class.java, BusyComplication::class.java)) runCatching { ComplicationDataSourceUpdateRequester.create(c, ComponentName(c, k)).requestUpdateAll() }
    }
}
