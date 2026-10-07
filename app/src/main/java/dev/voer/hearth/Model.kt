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
