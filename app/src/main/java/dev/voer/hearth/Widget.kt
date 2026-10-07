package dev.voer.hearth

import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.*
import android.os.Build
import android.widget.RemoteViews
import org.json.JSONObject
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.roundToInt
import kotlin.math.sin
import kotlin.math.sqrt

enum class Look(val label: String) { CLAUDE("Claude"), SYSTEM("System") }
enum class Show(val label: String) { SESSION("Session"), WEEK("Week"), BOTH("Both") }
enum class Shape(val label: String, val w: Float, val h: Float) {
    TINY("Mini", 72f, 72f), SMALL("Ring", 150f, 150f), WIDE("Dual", 310f, 150f), STRIP("Strip", 310f, 64f);

    companion object {
        /** Layout follows the size the widget is actually resized to, whichever provider it came from. */
        fun fit(w: Float, h: Float) = when {
            h < 90 && w < 110 -> TINY
            h < 90 -> STRIP
            w / h > 1.4f -> WIDE
            else -> SMALL
        }
    }
}

data class WCfg(
    val look: Look = Look.SYSTEM, val show: Show = Show.BOTH, val remaining: Boolean = false,
    val mode: Int = 0, val bgAlpha: Int = 100, val radius: Int = 26, val accent: Int = 0, val reset: Boolean = true,
    /** Mini widget content: 0 ring + percent, 1 ring only, 2 percent only. */
    val tiny: Int = 0,
) {
    fun toJson() = JSONObject().put("look", look.name).put("show", show.name).put("rem", remaining).put("mode", mode)
        .put("bg", bgAlpha).put("rad", radius).put("acc", accent).put("reset", reset).put("tiny", tiny).toString()

    companion object {
        fun from(s: String) = runCatching {
            val o = JSONObject(s)
            WCfg(Look.entries.firstOrNull { it.name == o.getString("look") } ?: Look.SYSTEM, Show.valueOf(o.getString("show")), o.getBoolean("rem"), o.getInt("mode"),
                o.getInt("bg"), o.getInt("rad"), o.getInt("acc"), o.getBoolean("reset"), o.optInt("tiny", 0))
        }.getOrNull()
    }
}

private fun a(c: Int, alpha: Int) = (c and 0x00FFFFFF) or (alpha shl 24)
private const val RED = 0xFFD71921.toInt()
private const val CLAY = 0xFFF0643C.toInt()

private class Pal(val bg: Int, val fg: Int, val accent: Int, val track: Int)

private fun palette(c: Context, cfg: WCfg): Pal {
    val dark = when (cfg.mode) {
        1 -> true; 2 -> false
        else -> (c.resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
    }
    return when {
        cfg.look == Look.SYSTEM && Build.VERSION.SDK_INT >= 31 -> {
            fun col(r: Int) = c.getColor(r)
            if (dark) Pal(col(android.R.color.system_neutral1_900), col(android.R.color.system_neutral1_50),
                col(android.R.color.system_accent1_200), col(android.R.color.system_neutral2_700))
            else Pal(col(android.R.color.system_neutral1_10), col(android.R.color.system_neutral1_900),
                col(android.R.color.system_accent1_600), col(android.R.color.system_accent1_100))
        }
        else -> if (dark) Pal(0xFF262624.toInt(), 0xFFFAF9F5.toInt(), CLAY, 0xFF3F3E3A.toInt())
        else Pal(0xFFF0EEE6.toInt(), 0xFF3D3929.toInt(), 0xFFC15F3C.toInt(), 0xFFDDD9CB.toInt())
    }
}

private class Renderer(val ctx: Context, val cfg: WCfg, val u: Usage?, val now: Long) {
    val pal = palette(ctx, cfg)
    val p = Paint(Paint.ANTI_ALIAS_FLAG)
    val dim = a(pal.fg, 0x99)
    val labelFace: Typeface = Typeface.create("sans-serif-medium", Typeface.NORMAL)
    val numFace: Typeface = if (cfg.look == Look.CLAUDE) Typeface.SERIF else Typeface.create("sans-serif-light", Typeface.NORMAL)

    fun metric(m: Show) = if (m == Show.SESSION) u?.session else u?.week
    fun used(m: Show) = metric(m)?.at(now)
    fun shown(v: Double) = if (cfg.remaining) 100 - v else v
    fun name(m: Show) = if (m == Show.SESSION) "Session" else "Week"
    fun accentColor() = when (cfg.accent) {
        1 -> CLAY; 2 -> RED; 3 -> pal.fg; 4 -> 0xFF4CAF7D.toInt(); 5 -> 0xFF5B8DEF.toInt(); else -> pal.accent
    }
    fun fill(): Int = if (cfg.accent != 0) accentColor() else pal.accent

    fun resetLine(m: Show): String {
        val l = metric(m) ?: return "Open app"
        val r = l.resetsAt
        if (r == null || r < now) return "Ready"
        return "Resets in ${dur(r - now)}"
    }

    fun text(c: Canvas, s: String, x: Float, y: Float, size: Float, color: Int, face: Typeface, align: Paint.Align = Paint.Align.LEFT) {
        p.style = Paint.Style.FILL; p.typeface = face; p.textSize = size; p.color = color; p.textAlign = align
        c.drawText(s, x, y, p)
        p.textAlign = Paint.Align.LEFT
    }

    fun number(c: Canvas, used: Double?, cx: Float, cy: Float, h0: Float) {
        val n = if (used == null) "--" else shown(used).roundToInt().toString()
        val h = if (n.length >= 3) h0 * .78f else h0
        p.typeface = numFace; p.textSize = h * 1.38f
        val w1 = p.measureText(n); p.textSize = h * .5f
        val w2 = p.measureText("%"); val gap = h * .06f
        val x0 = cx - (w1 + gap + w2) / 2
        text(c, n, x0, cy + h / 2, h * 1.38f, pal.fg, numFace)
        text(c, "%", x0 + w1 + gap, cy + h / 2, h * .5f, dim, numFace)
    }

    fun ring(c: Canvas, cx: Float, cy: Float, r: Float, used: Double?) {
        val frac = ((used ?: 0.0) / 100).coerceIn(0.0, 1.0)
        val sw = r * if (cfg.look == Look.SYSTEM) .24f else .15f
        p.style = Paint.Style.STROKE; p.strokeWidth = sw; p.strokeCap = Paint.Cap.ROUND
        val rect = RectF(cx - r, cy - r, cx + r, cy + r)
        p.color = pal.track; c.drawArc(rect, 0f, 360f, false, p)
        if (frac > 0) { p.color = fill(); c.drawArc(rect, -90f, (360 * frac).toFloat(), false, p) }
        p.style = Paint.Style.FILL; p.strokeCap = Paint.Cap.BUTT
    }

    fun bar(c: Canvas, l: Float, r: Float, cy: Float, h: Float, used: Double?) {
        val frac = ((used ?: 0.0) / 100).coerceIn(0.0, 1.0)
        val rr = h / 2
        p.style = Paint.Style.FILL
        p.color = pal.track; c.drawRoundRect(l, cy - rr, r, cy + rr, rr, rr, p)
        if (frac > 0) { p.color = fill(); c.drawRoundRect(l, cy - rr, l + (r - l) * frac.toFloat().coerceAtLeast(h / (r - l)), cy + rr, rr, rr, p) }
    }

    fun panel(c: Canvas, l: Float, t: Float, r: Float, b: Float, m: Show, bottom: String?) {
        val pad = 11f
        text(c, name(m), l + pad, t + pad + 8f, 10f, dim, labelFace)
        val ringTop = t + pad + 14; val ringBot = b - pad - 14
        val side = min(r - l - 2 * pad, ringBot - ringTop)
        val rad = side / 2 - side * .08f
        val cx = (l + r) / 2; val cy = (ringTop + ringBot) / 2
        val u1 = used(m)
        ring(c, cx, cy, rad, u1)
        number(c, u1, cx, cy, rad * .6f)
        val bt = bottom ?: if (cfg.reset) resetLine(m) else null
        if (bt != null) text(c, bt, cx, b - pad, 10f, dim, labelFace, Paint.Align.CENTER)
    }

    fun strip(c: Canvas, w: Float, h: Float, rows: List<Show>) {
        val pad = 12f
        val rowH = (h - 2 * pad) / rows.size
        rows.forEachIndexed { i, m ->
            val cy = pad + rowH * (i + .5f)
            val u1 = used(m)
            text(c, name(m), pad, cy + 3.5f, 10f, dim, labelFace)
            val pt = if (u1 == null) "--" else "${shown(u1).roundToInt()}%"
            text(c, pt, w - pad, cy + 5f, 14f, pal.fg, numFace, Paint.Align.RIGHT)
            bar(c, pad + 64f, w - pad - 44f, cy, min(7f, rowH * .4f), u1)
        }
    }

    fun wideSingle(c: Canvas, w: Float, h: Float, m: Show) {
        val side = min(h, w * .46f)
        panel(c, 0f, 0f, side, h, m, null)
        val x = side + 6; val pad = 12f
        val l = metric(m)
        text(c, "Resets in", x, pad + 9f, 10f, dim, labelFace)
        val r = l?.resetsAt
        text(c, if (r == null || r < now) "Ready" else dur(r - now), x, pad + 38f, 24f, pal.fg, numFace)
        val pc = pace(l, if (m == Show.SESSION) SESSION_MS else WEEK_MS, now)
        paceWord(l, pc)?.let { text(c, it, x, h - pad - 22f, 10f, pal.fg, labelFace) }
        val sub = when {
            pc?.hitsAt != null -> "Limit in ${dur(pc.hitsAt - now)}"
            pc != null -> "Ends near ${pc.projected.coerceAtMost(100.0).roundToInt()}%"
            else -> ""
        }
        text(c, sub, x, h - pad - 8f, 10f, dim, labelFace)
    }

    fun tiny(c: Canvas, w: Float, h: Float) {
        val cx = w / 2; val cy = h / 2; val side = min(w, h)
        val ringOn = cfg.tiny != 2; val numOn = cfg.tiny != 1
        val m = if (cfg.show == Show.WEEK) Show.WEEK else Show.SESSION
        val u1 = used(m)
        val outer = side / 2 - side * .13f
        if (ringOn) {
            ring(c, cx, cy, outer, u1)
            if (cfg.show == Show.BOTH && !numOn) ring(c, cx, cy, outer * .66f, used(Show.WEEK))
        }
        if (!numOn) return
        if (ringOn) number(c, u1, cx, cy, outer * .6f)
        else {
            val both = cfg.show == Show.BOTH
            number(c, u1, cx, cy - if (both) side * .08f else 0f, side * .4f)
            if (both) text(c, "Week " + (used(Show.WEEK)?.let { "${shown(it).roundToInt()}%" } ?: "--"),
                cx, cy + side * .36f, 9f, dim, labelFace, Paint.Align.CENTER)
        }
    }

    fun render(shape: Shape, wDp: Float, hDp: Float, scale: Float): Bitmap {
        val bmp = Bitmap.createBitmap((wDp * scale).roundToInt().coerceAtLeast(1), (hDp * scale).roundToInt().coerceAtLeast(1), Bitmap.Config.ARGB_8888)
        val c = Canvas(bmp); c.scale(scale, scale)
        val rad = min(cfg.radius.toFloat(), min(wDp, hDp) / 2)
        p.style = Paint.Style.FILL; p.color = a(pal.bg, (cfg.bgAlpha * 2.55f).roundToInt())
        c.drawRoundRect(0f, 0f, wDp, hDp, rad, rad, p)
        val rows = if (cfg.show == Show.BOTH) listOf(Show.SESSION, Show.WEEK) else listOf(cfg.show)
        when (shape) {
            Shape.TINY -> tiny(c, wDp, hDp)
            Shape.SMALL -> {
                val m = if (cfg.show == Show.WEEK) Show.WEEK else Show.SESSION
                val other = if (cfg.show == Show.BOTH) Show.WEEK else null
                val bottom = other?.let { o -> used(o)?.let { "Week ${shown(it).roundToInt()}%" } ?: "Week --" }
                panel(c, 0f, 0f, wDp, hDp, m, bottom)
            }
            Shape.WIDE -> if (rows.size == 2) {
                panel(c, 0f, 0f, wDp / 2, hDp, Show.SESSION, null)
                panel(c, wDp / 2, 0f, wDp, hDp, Show.WEEK, null)
            } else wideSingle(c, wDp, hDp, rows[0])
            Shape.STRIP -> strip(c, wDp, hDp, rows)
        }
        return bmp
    }
}

object WidgetUpdater {
    fun hasWidgets(c: Context): Boolean {
        val m = AppWidgetManager.getInstance(c)
        return providers.any { (cls, _) -> m.getAppWidgetIds(ComponentName(c, cls)).isNotEmpty() }
    }

    private val providers = listOf(TinyWidget::class.java to Shape.TINY, SmallWidget::class.java to Shape.SMALL, WideWidget::class.java to Shape.WIDE, StripWidget::class.java to Shape.STRIP)

    fun bitmap(c: Context, cfg: WCfg, shape: Shape, u: Usage?, wDp: Float, hDp: Float, density: Float): Bitmap {
        val px = wDp * density * hDp * density
        val scale = density * min(1f, sqrt(900_000f / px))
        return Renderer(c, cfg, u, System.currentTimeMillis()).render(Shape.fit(wDp, hDp), wDp, hDp, scale)
    }

    fun updateAll(c: Context) {
        val m = AppWidgetManager.getInstance(c)
        providers.forEach { (cls, shape) -> m.getAppWidgetIds(ComponentName(c, cls)).forEach { update(c, m, it, shape) } }
    }

    fun update(c: Context, m: AppWidgetManager, id: Int, shape: Shape) {
        val o = m.getAppWidgetOptions(id)
        val w = o.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_WIDTH).takeIf { it > 0 }?.toFloat() ?: shape.w
        val h = o.getInt(AppWidgetManager.OPTION_APPWIDGET_MIN_HEIGHT).takeIf { it > 0 }?.toFloat() ?: shape.h
        val u = Store.usage(c)
        val bmp = bitmap(c, Store.widget(c, id), shape, u, w, h, c.resources.displayMetrics.density)
        val rv = RemoteViews(c.packageName, R.layout.widget_img)
        rv.setImageViewBitmap(R.id.img, bmp)
        rv.setContentDescription(R.id.img, if (u == null) "Claude usage, signed out" else
            "Claude usage. Session ${u.session?.at(u.at)?.toInt() ?: 0} percent, week ${u.week?.at(u.at)?.toInt() ?: 0} percent")
        rv.setOnClickPendingIntent(R.id.root, PendingIntent.getActivity(c, 0, Intent(c, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE))
        m.updateAppWidget(id, rv)
    }
}

abstract class BaseWidget(private val shape: Shape) : AppWidgetProvider() {
    override fun onUpdate(c: Context, m: AppWidgetManager, ids: IntArray) {
        ids.forEach { WidgetUpdater.update(c, m, it, shape) }
        Sched.ensure(c); Sched.now(c); CallService.sync(c)
    }
    override fun onAppWidgetOptionsChanged(c: Context, m: AppWidgetManager, id: Int, o: android.os.Bundle) =
        WidgetUpdater.update(c, m, id, shape)
    override fun onDeleted(c: Context, ids: IntArray) { ids.forEach { Store.dropWidget(c, it) }; CallService.sync(c) }
    override fun onEnabled(c: Context) { Sched.ensure(c); CallService.sync(c) }
}

class TinyWidget : BaseWidget(Shape.TINY)
class SmallWidget : BaseWidget(Shape.SMALL)
class WideWidget : BaseWidget(Shape.WIDE)
class StripWidget : BaseWidget(Shape.STRIP)
