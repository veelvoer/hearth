package dev.voer.hearth

import org.json.JSONObject
import java.time.OffsetDateTime

const val SESSION_MS = 5 * 3600_000L
const val WEEK_MS = 7 * 24 * 3600_000L

data class Limit(val pct: Double, val resetsAt: Long?) {
    /** A window that already rolled over has nothing used yet. */
    fun rolled(t: Long) = resetsAt != null && resetsAt < t
    fun at(t: Long) = if (rolled(t)) 0.0 else pct
    fun toJson(): JSONObject = JSONObject().put("p", pct).put("r", resetsAt ?: JSONObject.NULL)

    companion object {
        fun from(o: JSONObject?) = o?.let { Limit(it.optDouble("p"), if (it.isNull("r")) null else it.getLong("r")) }
        fun fromApi(o: JSONObject?): Limit? {
            if (o == null || o.isNull("utilization")) return null
            val r = o.optString("resets_at").takeIf { it.isNotEmpty() && it != "null" }
                ?.let { runCatching { OffsetDateTime.parse(it).toInstant().toEpochMilli() }.getOrNull() }
            return Limit(o.getDouble("utilization"), r)
        }
    }
}

data class Extra(val used: Double, val limit: Double, val pct: Double)

data class Usage(
    val session: Limit?, val week: Limit?, val opus: Limit?, val sonnet: Limit?,
    val extra: Extra?, val at: Long,
) {
    fun toJson(): JSONObject = JSONObject().apply {
        session?.let { put("s", it.toJson()) }; week?.let { put("w", it.toJson()) }
        opus?.let { put("o", it.toJson()) }; sonnet?.let { put("n", it.toJson()) }
        extra?.let { put("x", JSONObject().put("u", it.used).put("l", it.limit).put("p", it.pct)) }
        put("t", at)
    }

    companion object {
        fun from(o: JSONObject) = Usage(
            Limit.from(o.optJSONObject("s")), Limit.from(o.optJSONObject("w")),
            Limit.from(o.optJSONObject("o")), Limit.from(o.optJSONObject("n")),
            o.optJSONObject("x")?.let { Extra(it.getDouble("u"), it.getDouble("l"), it.getDouble("p")) },
            o.getLong("t"),
        )

        fun fromApi(o: JSONObject, now: Long): Usage {
            val x = o.optJSONObject("extra_usage")
            val extra = if (x != null && x.optBoolean("is_enabled") && !x.isNull("monthly_limit"))
                Extra(x.optDouble("used_credits", 0.0) / 100, x.getDouble("monthly_limit") / 100, x.optDouble("utilization", 0.0))
            else null
            return Usage(
                Limit.fromApi(o.optJSONObject("five_hour")), Limit.fromApi(o.optJSONObject("seven_day")),
                Limit.fromApi(o.optJSONObject("seven_day_opus")), Limit.fromApi(o.optJSONObject("seven_day_sonnet")),
                extra, now,
            )
        }
    }
}

data class Profile(val email: String, val plan: String)

class Pace(val elapsed: Double, val projected: Double, val hitsAt: Long?)

/** Linear projection of the current burn rate across the whole window. */
fun pace(l: Limit?, window: Long, t: Long): Pace? {
    val r = l?.resetsAt ?: return null
    if (r < t) return null
    val start = r - window
    val el = (t - start).coerceIn(1, window)
    val frac = el.toDouble() / window
    if (frac < 0.04 || l.pct <= 0) return Pace(frac, l.pct, null)
    val proj = l.pct / frac
    return Pace(frac, proj, if (proj > 100) start + (el * 100.0 / l.pct).toLong() else null)
}

fun paceWord(l: Limit?, p: Pace?): String? {
    if (l == null || p == null) return null
    val delta = l.pct - p.elapsed * 100
    return when {
        delta > 5 -> "Ahead of pace"
        delta < -5 -> "Under pace"
        else -> "On pace"
    }
}

fun dur(ms: Long): String {
    val m = (ms / 60_000).coerceAtLeast(0)
    return when {
        m < 60 -> "${m}m"
        m < 24 * 60 -> "${m / 60}h ${m % 60}m"
        else -> "${m / 1440}d ${(m % 1440) / 60}h"
    }
}

/** Money value of the work: the last 30 days in total, and per day for the chart. */
data class ApiCost(val total: Double, val days: List<Double>)

/** One weekday of the weekly plan: what was used, what is planned in total, and whether you pinned it. */
data class PlanDay(val wd: Int, val name: String, val state: String, val used: Double, val planned: Double, val fixed: Boolean, val target: Double?)
data class PlanSeg(val wd: Int, val f: Double, val state: String, val used: Double, val w: Double)
data class WeekPlan(val unavailable: String?, val left: Double, val over: Boolean, val leftover: Double, val unknown: Double, val autoDays: Int, val autoEach: Double, val days: List<PlanDay>,
                    val segs: List<PlanSeg> = emptyList(), val targets: Map<Int, Double> = emptyMap(), val r: Double = 0.0)
data class Groups(val days: List<PlanDay>, val over: Boolean, val leftover: Double, val autoDays: Int, val autoEach: Double)

private fun r1(x: Double) = Math.round(x * 10) / 10.0

/** The same sharing as the computer does (weekplan.js): pinned days get their share, the rest of what is left is shared evenly over the days that are still to come.
 *  Used while a handle is dragged, so the other days follow at once. */
fun groupsOf(segs: List<PlanSeg>, targets: Map<Int, Double>, r: Double): Groups {
    fun tgt(wd: Int): Double? = targets[wd]?.coerceIn(0.0, 100.0)
    var fixedWant = 0.0; var autoW = 0.0
    val want = DoubleArray(segs.size) { -1.0 }
    segs.forEachIndexed { i, s ->
        if (s.state == "past") return@forEachIndexed
        val t = tgt(s.wd)
        if (t != null) { want[i] = maxOf(0.0, t * s.f - (if (s.state == "today") s.used else 0.0)); fixedWant += want[i] } else autoW += s.w
    }
    val over = fixedWant > r + 1e-9
    val k = if (over && fixedWant > 0) r / fixedWant else 1.0
    val pool = maxOf(0.0, r - fixedWant)
    val alloc = DoubleArray(segs.size) { i -> val s = segs[i]; if (s.state == "past") 0.0 else if (want[i] >= 0) want[i] * k else if (over || autoW <= 0) 0.0 else pool * s.w / autoW }
    val leftover = if (autoW <= 0 && !over) maxOf(0.0, r - fixedWant) else 0.0
    val order = segs.map { it.wd }.distinct()
    val names = listOf("Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat")
    val days = order.map { wd ->
        val idx = segs.indices.filter { segs[it].wd == wd }
        val t = tgt(wd)
        val state = if (idx.any { segs[it].state == "today" }) "today" else if (idx.all { segs[it].state == "past" }) "past" else "future"
        val used = idx.sumOf { segs[it].used }
        val planned = idx.sumOf { segs[it].used + (if (segs[it].state == "past") 0.0 else alloc[it]) }
        PlanDay(wd, names[wd], state, r1(used), r1(planned), t != null, t)
    }
    val autoDays = segs.indices.filter { segs[it].state != "past" && want[it] < 0 }.map { segs[it].wd }.distinct().size
    return Groups(days, over, r1(leftover), autoDays, if (autoW > 0 && !over) r1(pool / autoW) else 0.0)
}

