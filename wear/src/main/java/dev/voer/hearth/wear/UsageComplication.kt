package dev.voer.hearth.wear

import android.app.PendingIntent
import android.content.Intent
import androidx.wear.watchface.complications.data.ComplicationData
import androidx.wear.watchface.complications.data.ComplicationType
import androidx.wear.watchface.complications.data.PlainComplicationText
import androidx.wear.watchface.complications.data.RangedValueComplicationData
import androidx.wear.watchface.complications.data.ShortTextComplicationData
import androidx.wear.watchface.complications.datasource.ComplicationRequest
import androidx.wear.watchface.complications.datasource.SuspendingComplicationDataSourceService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** The 5-hour usage as a complication (any watch face): a ring or a short "72%", a tap opens Hearth. */
class UsageComplication : SuspendingComplicationDataSourceService() {
    private fun tap() = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)

    override fun getPreviewData(type: ComplicationType): ComplicationData? = build(type, 42.0)

    override suspend fun onComplicationRequest(request: ComplicationRequest): ComplicationData? {
        Phone.app = applicationContext
        val v = withContext(Dispatchers.IO) { Servers.all(this@UsageComplication).firstNotNullOfOrNull { s -> runCatching { Api.usage(s).session }.getOrNull() } }
        return build(request.complicationType, v)
    }

    private fun build(type: ComplicationType, v: Double?): ComplicationData? {
        val txt = PlainComplicationText.Builder(v?.let { "${it.toInt()}%" } ?: "–").build()
        val name = PlainComplicationText.Builder("Claude usage").build()
        return when (type) {
            ComplicationType.RANGED_VALUE -> RangedValueComplicationData.Builder((v ?: 0.0).toFloat().coerceIn(0f, 100f), 0f, 100f, name).setText(txt).setTapAction(tap()).build()
            ComplicationType.SHORT_TEXT -> ShortTextComplicationData.Builder(txt, name).setTapAction(tap()).build()
            else -> null
        }
    }
}
