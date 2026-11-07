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
import androidx.compose.ui.graphics.Color
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.json.JSONObject

/** One entry of the settings menu: where it goes, what it looks like, and the words the search looks in. */
private data class SPage(val id: String, val icon: String, val title: String, val sub: String, val words: String, val short: String = title)

private val PAGES = listOf(
    SPage("computers", "server", "Computers and servers", "Where your chats run", "connect add remove pair address token wifi", "Computers"),
    SPage("updates", "sync", "Updates", "Newest version, what changed", "update version download install release"),
    SPage("notifications", "bell", "Notifications and alerts", "Calls, usage alerts, background", "notify call alert battery background threshold refresh sound status bar", "Alerts"),
    SPage("look", "contrast", "Look and feel", "Colors, Home, widgets, voice", "theme dark light color system glass animation widget voice language dashboard home", "Look"),
    SPage("routines", "clock", "Routines", "Goals Claude works on by itself", "routine schedule agent automatic daily digest"),
    SPage("server", "chip", "Server", "Sync, memory, usage limit, safety", "sync memory limit continue restart safety backup budget snapshot undo rules"),
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
    SettingsGroup { row("routines", false)(); row("server", false)(); row("tools", true)() }
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
    Fun("co2", "cloud", "CO₂", "A rough guess of the CO₂ behind the electricity, shown as meters driven in a car."),
    Fun("timesaved", "clock", "Time saved", "How long it would have taken to type what Claude wrote, at 40 words a minute."),
    Fun("bulb", "bulb", "Light bulb", "The energy used, as hours of a 10 watt LED bulb."),
    Fun("bottles", "drop", "Water bottles", "The water estimate, as bottles of half a litre."),
    Fun("pizza", "pizza", "Pizza money", "What your Claude work would have cost at API prices, in pizzas of 15 dollars."),
    Fun("perprompt", "coin", "Cost per prompt", "What one prompt of yours is worth on average, at API prices."),
)
/** Just for fun: nothing to learn here. */
val TOYS = listOf(
    Fun("confetti", "party", "Confetti", "A burst of confetti when Claude finishes."),
    Fun("sounds", "volume", "Little sounds", "A tiny beep when you send and when Claude is done."),
    Fun("pep", "heart", "Daily pep talk", "A kind line on Home, a new one every day."),
    Fun("hello", "globe", "Hello, world", "Home greets you in another language each day."),
    Fun("ember", "fire", "Ember", "A little flame on Home. Poke it."),
    Fun("verbs", "sparkle", "Playful status words", "Words like Pondering… and Flibbertigibbeting… while Claude is working."),
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

private fun fmtN(v: Double): String = if (v >= 1e9) "%.1fB".format(java.util.Locale.US, v / 1e9) else if (v >= 1e6) "%.1fM".format(java.util.Locale.US, v / 1e6) else if (v >= 1e3) "%.1fk".format(java.util.Locale.US, v / 1e3) else v.toInt().toString()

/** What a beta feature says about the last 30 days: (big value, small line), or null when there is nothing to say yet. */
fun funLine(id: String, o: JSONObject): Pair<String, String>? {
    val t = o.optJSONObject("totals") ?: return null
    val out = t.optDouble("out", 0.0)
    return when (id) {
        "water" -> out.takeIf { it > 0 }?.let { val ml = it / 1000 * 2; ("≈ " + (if (ml < 1000) "${ml.toInt()} ml" else "%.1f L".format(java.util.Locale.US, ml / 1000))) to "A rough guess: about 2 ml per 1,000 tokens written" }
        "energy" -> out.takeIf { it > 0 }?.let { val wh = it / 1000 * 0.3; ("≈ " + (if (wh < 1000) "%.0f Wh".format(java.util.Locale.US, wh) else "%.1f kWh".format(java.util.Locale.US, wh / 1000))) to "About ${"%.1f".format(java.util.Locale.US, wh / 15)} phone charges" }
        "coffee" -> t.optDouble("cost", 0.0).takeIf { it > 0 }?.let { "≈ ${"%.0f".format(java.util.Locale.US, it / 4)} cups" to "At API prices of \$${"%,.0f".format(java.util.Locale.US, it)}, a coffee at \$4" }
        "words" -> out.takeIf { it > 0 }?.let { val w = it * 0.75; ("≈ " + fmtN(w) + " words") to "About ${"%,.0f".format(java.util.Locale.US, w / 300)} book pages" }
        "streak" -> t.optInt("currentStreak", 0).let { "$it day${if (it == 1) "" else "s"}" to "Best streak: ${t.optInt("longestStreak", 0)} days" }
        "favorite" -> o.optString("favorite").takeIf { it.isNotBlank() }?.let { it to "Did most of your work" }
        "busiest" -> o.optJSONArray("days")?.let { a -> (0 until a.length()).map { a.getJSONObject(it) }.maxByOrNull { it.optDouble("tokens", 0.0) }?.takeIf { it.optDouble("tokens", 0.0) > 0 }?.let { d ->
            runCatching { java.time.LocalDate.parse(d.getString("date")).format(java.time.format.DateTimeFormatter.ofPattern("EEE d MMM")) }.getOrDefault(d.optString("date")) to "${fmtN(d.optDouble("tokens", 0.0))} tokens that day" } }
        "weekend" -> o.optJSONArray("days")?.let { a ->
            var we = 0.0; var all = 0.0
            for (i in 0 until a.length()) { val d = a.getJSONObject(i); val tk = d.optDouble("tokens", 0.0); all += tk; val dow = runCatching { java.time.LocalDate.parse(d.getString("date")).dayOfWeek.value }.getOrDefault(1); if (dow >= 6) we += tk }
            if (all > 0) "${(we / all * 100).toInt()}%" to "Of your work is on a Saturday or Sunday" else null }
        "longest" -> t.optLong("longestSessionMs", 0).takeIf { it > 0 }?.let { ms -> val m = ms / 60000; (if (m >= 1440) "${m / 1440}d ${m % 1440 / 60}h" else if (m >= 60) "${m / 60}h ${m % 60}m" else "${m}m") to "Your longest session in one go" }
        "co2" -> out.takeIf { it > 0 }?.let { val g = it / 1000 * 0.3 * 0.4; (if (g < 1000) "≈ ${"%.0f".format(java.util.Locale.US, g)} g" else "≈ ${"%.1f".format(java.util.Locale.US, g / 1000)} kg") to "Like driving ${"%,.0f".format(java.util.Locale.US, g / 0.12)} meters in a car" }
        "timesaved" -> out.takeIf { it > 0 }?.let { val h = it * 0.75 / 40 / 60; (if (h < 1) "≈ ${"%.0f".format(java.util.Locale.US, h * 60)} min" else "≈ ${"%.1f".format(java.util.Locale.US, h)} hours") to "Of typing, at 40 words a minute" }
        "bulb" -> out.takeIf { it > 0 }?.let { val hrs = it / 1000 * 0.3 / 10; (if (hrs < 1) "≈ ${"%.0f".format(java.util.Locale.US, hrs * 60)} min" else "≈ ${"%.1f".format(java.util.Locale.US, hrs)} hours") to "Of a 10 watt LED bulb" }
        "bottles" -> out.takeIf { it > 0 }?.let { val b = it / 1000 * 2 / 500; "≈ ${"%.1f".format(java.util.Locale.US, b)} bottles" to "Of half a litre each" }
        "pizza" -> t.optDouble("cost", 0.0).takeIf { it > 0 }?.let { "≈ ${"%.1f".format(java.util.Locale.US, it / 15)} pizzas" to "At \$15 a pizza" }
        "perprompt" -> if (t.optInt("prompts", 0) > 0 && t.optDouble("cost", 0.0) > 0) "\$${"%.2f".format(java.util.Locale.US, t.optDouble("cost", 0.0) / t.optInt("prompts", 1))}" to "Per prompt, on average, at API prices" else null
        "cache" -> t.optDouble("cacheSaved", 0.0).takeIf { it > 0 }?.let { "\$${"%,.0f".format(java.util.Locale.US, it)}" to "Saved by caching, at API prices" }
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
    Label("Fun facts · last 30 days")
    on.chunked(2).forEach { pair ->   // two tiles to a row: a big number, what it is, and a small line
        Row(Modifier.fillMaxWidth().height(IntrinsicSize.Max), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            pair.forEach { (f, v) ->
                Column(Modifier.weight(1f).fillMaxHeight().appear().panel(RoundedCornerShape(18.dp)).padding(14.dp)) {
                    Box(Modifier.size(32.dp).clip(RoundedCornerShape(10.dp)).background(softFill()), contentAlignment = Alignment.Center) { Glyph(f.icon, 18.dp, MaterialTheme.colorScheme.primary) }
                    Spacer(Modifier.height(8.dp))
                    Txt(v.first, T.h2.copy(fontSize = 20.sp))
                    Txt(f.title, T.small.copy(fontWeight = FontWeight.SemiBold), maxLines = 1)
                    Txt(v.second, T.small.copy(fontSize = 12.sp, lineHeight = 16.sp), muted = true)
                }
            }
            if (pair.size == 1) Spacer(Modifier.weight(1f))
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

/** A quick health check of the server: what works, what does not, and what to do about it. */
@Composable
fun DoctorCard() {
    val c = LocalContext.current
    val scope = rememberCoroutineScope()
    var rows by remember { mutableStateOf<List<List<Any>>?>(null) }
    var busy by remember { mutableStateOf(false) }
    var err by remember { mutableStateOf("") }
    Card {
        Head("shield", "Health check", "Looks at Claude Code, the projects folder, disk space and the chat files of your server, and says what to do about anything that is wrong.") {
            TextAction("Check") {
                if (busy) return@TextAction
                val ms = Store.machines(c); val m = ms.firstOrNull { it.secure } ?: ms.firstOrNull()
                if (m == null) { err = "Connect a server first."; return@TextAction }
                busy = true; err = ""
                scope.launch { runCatching { withContext(kotlinx.coroutines.Dispatchers.IO) { Relay.doctor(m) } }.onSuccess { rows = it }.onFailure { err = if ((it.message ?: "").let { x -> x.contains("404") || x.contains("unknown", true) || x.contains("not found", true) }) "This server is too old for the health check. Update it first." else friendlyError(it, m.name) }; busy = false }
            }
        }
        if (busy) { Spacer(Modifier.height(10.dp)); Skeleton(48.dp, radius = 14.dp) }
        if (err.isNotBlank()) { Spacer(Modifier.height(8.dp)); Txt(err, T.small, color = MaterialTheme.colorScheme.primary) }
        rows?.forEach { r ->
            val ok = r[1] as Boolean
            Spacer(Modifier.height(10.dp))
            Row(verticalAlignment = Alignment.Top) {
                Box(Modifier.padding(top = 3.dp).size(22.dp).clip(RoundedCornerShape(50)).background(if (ok) Color(0xFF4CAF7D).copy(alpha = .18f) else MaterialTheme.colorScheme.primary.copy(alpha = .18f)), contentAlignment = Alignment.Center) { Glyph(if (ok) "check" else "x", 14.dp, if (ok) Color(0xFF2E8B57) else MaterialTheme.colorScheme.primary) }
                Spacer(Modifier.width(12.dp))
                Column(Modifier.weight(1f)) {
                    Txt(r[0] as String, T.body.copy(fontWeight = FontWeight.SemiBold))
                    Txt(r[2] as String, T.small, muted = true)
                    if ((r[3] as String).isNotBlank()) Txt(r[3] as String, T.small, color = MaterialTheme.colorScheme.primary)
                }
            }
        }
    }
}
