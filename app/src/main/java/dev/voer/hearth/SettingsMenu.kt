package dev.voer.hearth

import android.content.Intent
import android.net.Uri
import android.os.Build
import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import org.json.JSONObject

/** One entry of the settings menu: where it goes, what it looks like, and the words the search looks in. */
private data class SPage(val id: String, val icon: String, val title: String, val sub: String, val words: String, val short: String = title)

private val PAGES = listOf(
    SPage("computers", "server", "Computers and servers", "Where your chats run", "connect add remove pair address token wifi", "Computers"),
    SPage("updates", "sync", "Updates", "Newest version, what changed", "update version download install release"),
    SPage("notifications", "bell", "Notifications and alerts", "Calls, usage alerts, background", "notify call alert battery background threshold refresh sound status bar", "Alerts"),
    SPage("look", "contrast", "Look and feel", "Colors, Home, widgets, voice", "theme dark light color system glass animation widget voice language dashboard home", "Look"),
    SPage("server", "chip", "Server", "Sync, memory, usage limit", "sync memory limit continue restart"),
    SPage("tools", "sparkle", "Tools for Claude", "Skills, MCP, API keys, permissions", "skill mcp api key permission root", "Tools"),
    SPage("help", "support", "Help and support", "Tour, setup, contact us", "tour tutorial setup support email contact bug", "Help"),
)

fun settingsTitle(id: String): String = PAGES.firstOrNull { it.id == id }?.title ?: if (id == "beta") "Beta features" else if (id == "about") "About" else "Settings"

/** The first page of Settings: a menu of rounded groups, like the settings of other apps. */
@Composable
fun SettingsMenu(open: (String) -> Unit) {
    val c = LocalContext.current
    var find by remember { mutableStateOf("") }
    val beta = Store.betaUnlocked(c)
    Header("Settings")
    Field(find, { find = it }, "Find a setting")
    val q = find.trim().lowercase()
    if (q.isNotEmpty()) {
        val hits = (PAGES + listOfNotNull(if (beta) SPage("beta", "flask", "Beta features", "Extras you can switch on", "beta fun water energy") else null, SPage("about", "info", "About", "Version and links", "about version logo credits")))
            .filter { (it.title + " " + it.sub + " " + it.words).lowercase().contains(q) }
        if (hits.isEmpty()) Txt("Nothing matches \"$find\".", T.small, muted = true)
        else SettingsGroup { hits.forEachIndexed { i, p -> SettingsRow(p.icon, p.title, p.sub, i == hits.lastIndex) { open(p.id) } } }
        return
    }
    fun row(id: String, last: Boolean) = PAGES.first { it.id == id }.let { p -> @Composable { SettingsRow(p.icon, p.title, p.sub, last) { open(p.id) } } }
    SettingsGroup { row("computers", false)(); row("updates", true)() }
    SettingsGroup { row("notifications", false)(); row("look", true)() }
    SettingsGroup { row("server", false)(); row("tools", true)() }
    SettingsGroup { row("help", true)() }
    SettingsGroup {
        if (beta) SettingsRow("flask", "Beta features", "Extras you can switch on", false) { open("beta") }
        SettingsRow("info", "About", "Hearth ${Updates.current(c)}", true) { open("about") }
    }
}

@Composable
private fun SettingsGroup(content: @Composable ColumnScope.() -> Unit) {
    Column(Modifier.appear().fillMaxWidth().panel(RoundedCornerShape(20.dp)), content = content)
}

@Composable
private fun SettingsRow(icon: String, title: String, sub: String, last: Boolean, onClick: () -> Unit) {
    val cs = MaterialTheme.colorScheme
    Row(Modifier.fillMaxWidth().clickable(role = Role.Button, onClick = onClick).heightIn(min = 68.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(40.dp).clip(RoundedCornerShape(12.dp)).background(softFill()), contentAlignment = Alignment.Center) { Glyph(icon, 22.dp, cs.primary) }
        Spacer(Modifier.width(14.dp))
        Column(Modifier.weight(1f).padding(vertical = 10.dp)) {
            Txt(title, T.body.copy(fontWeight = FontWeight.SemiBold), maxLines = 1)
            Txt(sub, T.small, muted = true, maxLines = 1)
        }
        Glyph("chevr", 22.dp)
    }
    if (!last) Box(Modifier.fillMaxWidth().padding(start = 70.dp).height(1.dp).background(cs.outline))
}

// ───────────────────────── About ─────────────────────────

@Composable
fun AboutPage(onBeta: () -> Unit) {
    val c = LocalContext.current
    val hap = LocalHapticFeedback.current
    var taps by remember { mutableIntStateOf(0) }
    var last by remember { mutableLongStateOf(0L) }
    Column(Modifier.fillMaxWidth().padding(vertical = 8.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.size(112.dp).clip(RoundedCornerShape(32.dp)).background(softFill())
            .clickable(role = Role.Button, onClickLabel = "Hearth logo") {
                val now = System.currentTimeMillis(); taps = if (now - last > 2500) 1 else taps + 1; last = now
                hap.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                when {
                    taps >= 10 -> { taps = 0; hap.performHapticFeedback(HapticFeedbackType.LongPress)
                        if (Store.betaUnlocked(c)) Toast.makeText(c, "Beta features are already on.", Toast.LENGTH_SHORT).show()
                        else { Store.setBetaUnlocked(c, true); Toast.makeText(c, "Beta features unlocked. Find them in Settings.", Toast.LENGTH_LONG).show(); onBeta() } }
                    taps >= 5 && !Store.betaUnlocked(c) -> Toast.makeText(c, "${10 - taps} more taps…", Toast.LENGTH_SHORT).show()
                }
            }, contentAlignment = Alignment.Center) { Spark(72.dp) }
        Spacer(Modifier.height(14.dp))
        Txt("Hearth", T.title)
        var vt by remember { mutableIntStateOf(0) }
        Txt("Version ${Updates.current(c)}", T.small, muted = true, modifier = Modifier.clickable(interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() }, indication = null) {
            vt++; if (vt >= 7) { vt = 0; Ui.burst++; Toast.makeText(c, "Made with warmth, a lot of coffee, and Claude.", Toast.LENGTH_LONG).show() }
        })
        Spacer(Modifier.height(6.dp))
        Txt("Claude Code, on your phone and your computer.", T.body, muted = true, modifier = Modifier.padding(horizontal = 12.dp))
    }
    val link = { url: String -> runCatching { c.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url))) }; Unit }
    SettingsGroup {
        InfoRow("Version", Updates.current(c))
        InfoRow("Computers connected", Store.machines(c).size.toString())
        InfoRow("Android", "${Build.VERSION.RELEASE} · ${Build.MANUFACTURER} ${Build.MODEL}")
        InfoRow("Made by", "Warre and Claude", last = true)
    }
    SettingsGroup {
        SettingsRow("link", "Source code", "github.com/veelvoer/hearth", false) { link("https://github.com/veelvoer/hearth") }
        SettingsRow("history", "Release notes", "What changed in every version", false) { link("https://github.com/veelvoer/hearth/releases") }
        SettingsRow("mail", "Contact", "hearth.support1@gmail.com", true) { link("mailto:hearth.support1@gmail.com") }
    }
    Txt("Hearth is an independent app, not made by or affiliated with Anthropic. Your usage limits come from what Claude Code reports on your own computer or server; Hearth never sees your Claude login.", T.small, muted = true, modifier = Modifier.padding(horizontal = 4.dp))
}

@Composable
private fun InfoRow(label: String, value: String, last: Boolean = false) {
    Row(Modifier.fillMaxWidth().heightIn(min = 52.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        Txt(label, T.body, modifier = Modifier.weight(1f))
        Spacer(Modifier.width(12.dp))
        Txt(value, T.small, muted = true, maxLines = 2)
    }
    if (!last) Box(Modifier.fillMaxWidth().padding(start = 16.dp).height(1.dp).background(MaterialTheme.colorScheme.outline))
}

// ───────────────────────── Beta features ─────────────────────────

/** A beta feature: a small extra you can switch on. Each one shows a line of its own on Home. */
data class Fun(val id: String, val icon: String, val title: String, val about: String)

val FUN = listOf(
    Fun("water", "drop", "Water used", "A rough guess of the water that cooling the data centres took for what Claude wrote."),
    Fun("energy", "bolt", "Energy used", "The electricity behind your replies, shown as phone charges."),
    Fun("coffee", "cup", "Coffee money", "What your Claude work would have cost at API prices, counted in cups of coffee."),
    Fun("words", "book", "Words written", "How much Claude wrote for you, in words and book pages."),
    Fun("streak", "fire", "Streak", "How many days in a row you used Claude Code."),
    Fun("favorite", "sparkle", "Favourite model", "The model that did most of your work."),
    Fun("busiest", "clock", "Busiest day", "The day you used Claude the most."),
    Fun("weekend", "home", "Weekend share", "How much of your work happens on a Saturday or Sunday."),
    Fun("longest", "history", "Longest chat", "Your longest session in one go."),
    Fun("cache", "chip", "Cache savings", "What caching saved you, at API prices."),
)
/** Just for fun: nothing to learn here. */
val TOYS = listOf(
    Fun("confetti", "party", "Confetti", "A burst of confetti when Claude finishes."),
    Fun("sounds", "volume", "Little sounds", "A tiny beep when you send and when Claude is done."),
    Fun("pep", "heart", "Daily pep talk", "A kind line on Home, a new one every day."),
    Fun("hello", "globe", "Hello, world", "Home greets you in another language each day."),
    Fun("ember", "fire", "Ember", "A little flame on Home. Poke it."),
)

@Composable
fun BetaPage() {
    val c = LocalContext.current
    @Composable fun group(list: List<Fun>) = SettingsGroup {
        list.forEachIndexed { i, f ->
            var on by remember { mutableStateOf(Store.betaOn(c, f.id)) }
            Row(Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(40.dp).clip(RoundedCornerShape(12.dp)).background(softFill()), contentAlignment = Alignment.Center) { Glyph(f.icon, 22.dp, MaterialTheme.colorScheme.primary) }
                Spacer(Modifier.width(14.dp))
                Txt(f.title, T.body.copy(fontWeight = FontWeight.SemiBold), modifier = Modifier.weight(1f))
                InfoDot(f.title, f.about)
                Switch(on, { on = it; Store.setBetaOn(c, f.id, it) })
            }
            if (i < list.lastIndex) Box(Modifier.fillMaxWidth().padding(start = 70.dp).height(1.dp).background(MaterialTheme.colorScheme.outline))
        }
    }
    Label("Numbers"); group(FUN)
    Label("Just for fun"); group(TOYS)
    androidx.compose.runtime.CompositionLocalProvider(LocalIconAlign provides Alignment.CenterStart) {
        TextAction("Hide beta features") { Store.setBetaUnlocked(c, false); (FUN + TOYS).forEach { Store.setBetaOn(c, it.id, false) }; Toast.makeText(c, "Beta features are hidden again.", Toast.LENGTH_SHORT).show() }
    }
}

private fun fmtN(v: Double): String = if (v >= 1e9) "%.1fB".format(v / 1e9) else if (v >= 1e6) "%.1fM".format(v / 1e6) else if (v >= 1e3) "%.1fk".format(v / 1e3) else v.toInt().toString()

/** What a beta feature says about the last 30 days: (big value, small line), or null when there is nothing to say yet. */
fun funLine(id: String, o: JSONObject): Pair<String, String>? {
    val t = o.optJSONObject("totals") ?: return null
    val out = t.optDouble("out", 0.0)
    return when (id) {
        "water" -> out.takeIf { it > 0 }?.let { val ml = it / 1000 * 2; ("≈ " + (if (ml < 1000) "${ml.toInt()} ml" else "%.1f L".format(ml / 1000))) to "A rough guess: about 2 ml per 1,000 tokens written" }
        "energy" -> out.takeIf { it > 0 }?.let { val wh = it / 1000 * 0.3; ("≈ " + (if (wh < 1000) "%.0f Wh".format(wh) else "%.1f kWh".format(wh / 1000))) to "About ${"%.1f".format(wh / 15)} phone charges" }
        "coffee" -> t.optDouble("cost", 0.0).takeIf { it > 0 }?.let { "≈ ${"%.0f".format(it / 4)} cups" to "At API prices of \$${"%,.0f".format(it)}, a coffee at \$4" }
        "words" -> out.takeIf { it > 0 }?.let { val w = it * 0.75; ("≈ " + fmtN(w) + " words") to "About ${"%,.0f".format(w / 300)} book pages" }
        "streak" -> t.optInt("currentStreak", 0).let { "$it day${if (it == 1) "" else "s"}" to "Best streak: ${t.optInt("longestStreak", 0)} days" }
        "favorite" -> o.optString("favorite").takeIf { it.isNotBlank() }?.let { it to "Did most of your work" }
        "busiest" -> o.optJSONArray("days")?.let { a -> (0 until a.length()).map { a.getJSONObject(it) }.maxByOrNull { it.optDouble("tokens", 0.0) }?.takeIf { it.optDouble("tokens", 0.0) > 0 }?.let { d ->
            runCatching { java.time.LocalDate.parse(d.getString("date")).format(java.time.format.DateTimeFormatter.ofPattern("EEE d MMM")) }.getOrDefault(d.optString("date")) to "${fmtN(d.optDouble("tokens", 0.0))} tokens that day" } }
        "weekend" -> o.optJSONArray("days")?.let { a ->
            var we = 0.0; var all = 0.0
            for (i in 0 until a.length()) { val d = a.getJSONObject(i); val tk = d.optDouble("tokens", 0.0); all += tk; val dow = runCatching { java.time.LocalDate.parse(d.getString("date")).dayOfWeek.value }.getOrDefault(1); if (dow >= 6) we += tk }
            if (all > 0) "${(we / all * 100).toInt()}%" to "Of your work is on a Saturday or Sunday" else null }
        "longest" -> t.optLong("longestSessionMs", 0).takeIf { it > 0 }?.let { ms -> val m = ms / 60000; (if (m >= 60) "${m / 60}h ${m % 60}m" else "${m}m") to "Your longest session in one go" }
        "cache" -> t.optDouble("cacheSaved", 0.0).takeIf { it > 0 }?.let { "\$${"%,.0f".format(it)}" to "Saved by caching, at API prices" }
        else -> null
    }
}

/** The lines of the beta features that are switched on, as one card on Home. */
@Composable
fun FunFactsCard(raw: JSONObject?) {
    val c = LocalContext.current
    if (!Store.betaUnlocked(c) || raw == null) return
    val on = FUN.filter { Store.betaOn(c, it.id) }.mapNotNull { f -> funLine(f.id, raw)?.let { f to it } }
    if (on.isEmpty()) return
    Card {
        Label("Fun facts · last 30 days")
        on.forEachIndexed { i, (f, v) ->
            Spacer(Modifier.height(if (i == 0) 8.dp else 12.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                Box(Modifier.size(40.dp).clip(RoundedCornerShape(12.dp)).background(softFill()), contentAlignment = Alignment.Center) { Glyph(f.icon, 22.dp, MaterialTheme.colorScheme.primary) }
                Spacer(Modifier.width(14.dp))
                Column(Modifier.weight(1f)) { Txt(v.first, T.h2.copy(fontSize = 20.sp)); Txt(f.title + " · " + v.second, T.small, muted = true) }
            }
        }
    }
}

// ───────────────────────── Full dashboard ─────────────────────────

/** Everything the computer's dashboard shows, in one scrolling block: totals, the models, the projects and the tools of the last 30 days. */
@Composable
fun FullDashboard(raw: JSONObject?) {
    if (raw == null) return
    val t = raw.optJSONObject("totals") ?: return
    Label("Full dashboard · last 30 days")
    Row(Modifier.fillMaxWidth().height(IntrinsicSize.Max), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        DashTile("Tokens", fmtN(t.optDouble("tokens", 0.0)), "written, read and cached", Modifier.weight(1f).fillMaxHeight())
        DashTile("Prompts", t.optInt("prompts", 0).toString(), "sent by you", Modifier.weight(1f).fillMaxHeight())
    }
    Row(Modifier.fillMaxWidth().height(IntrinsicSize.Max), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        DashTile("Sessions", t.optInt("sessions", 0).toString(), "${t.optInt("activeDays", 0)} active days", Modifier.weight(1f).fillMaxHeight())
        DashTile("Streak", "${t.optInt("currentStreak", 0)} days", "best ${t.optInt("longestStreak", 0)}", Modifier.weight(1f).fillMaxHeight())
    }
    val models = raw.optJSONArray("models")?.let { a -> (0 until a.length()).map { a.getJSONObject(it) } }.orEmpty().filter { it.optDouble("cost", 0.0) > 0 }
    if (models.isNotEmpty()) Card {
        Label("By model")
        val top = models.maxOf { it.optDouble("cost", 0.0) }.coerceAtLeast(0.0001)
        models.take(6).forEach { m -> DashBar(m.optString("label", m.optString("model")), m.optDouble("cost", 0.0) / top, "\$" + "%,.2f".format(m.optDouble("cost", 0.0))) }
    }
    val projects = raw.optJSONArray("projects")?.let { a -> (0 until a.length()).map { a.getJSONObject(it) } }.orEmpty().filter { it.optDouble("cost", 0.0) > 0 }
    if (projects.isNotEmpty()) Card {
        Label("By project")
        val top = projects.maxOf { it.optDouble("cost", 0.0) }.coerceAtLeast(0.0001)
        projects.take(6).forEach { p -> DashBar(p.optString("cwd").trimEnd('/').substringAfterLast('/').ifEmpty { "Other" }, p.optDouble("cost", 0.0) / top, "\$" + "%,.2f".format(p.optDouble("cost", 0.0))) }
    }
    val tools = raw.optJSONObject("tools")?.let { o -> o.keys().asSequence().map { it to o.optInt(it) }.toList() }.orEmpty().sortedByDescending { it.second }
    if (tools.isNotEmpty()) Card {
        Label("Most used tools")
        val top = tools.first().second.coerceAtLeast(1)
        tools.take(6).forEach { (n, v) -> DashBar(n, v.toDouble() / top, v.toString()) }
    }
    val saved = t.optDouble("cacheSaved", 0.0)
    if (saved > 0) Card { Label("Caching"); Txt("Saved you about \$" + "%,.0f".format(saved) + " at API prices.", T.body) }
}

@Composable
private fun DashTile(label: String, value: String, sub: String, modifier: Modifier) {
    Column(modifier.panel(RoundedCornerShape(18.dp)).padding(14.dp)) {
        Txt(label, T.small.copy(fontSize = 13.sp), muted = true)
        Txt(value, T.title.copy(fontSize = 26.sp))
        Txt(sub, T.small.copy(fontSize = 13.sp), muted = true)
    }
}

@Composable
private fun DashBar(label: String, frac: Double, value: String) {
    Spacer(Modifier.height(10.dp))
    Row { Txt(label, T.body, modifier = Modifier.weight(1f), maxLines = 1); Spacer(Modifier.width(8.dp)); Txt(value, T.body, muted = true) }
    Meter((frac * 100).toFloat().coerceIn(1f, 100f), height = 5.dp)
}
