package dev.voer.hearth

import android.content.Context
import android.media.AudioManager
import android.media.ToneGenerator
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import androidx.compose.ui.window.PopupProperties
import kotlinx.coroutines.delay
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin
import kotlin.random.Random

/** The small, unnecessary, fun extras of the beta page. */
object Toys {
    val PEP = listOf(
        "You are doing great. The tests are the ones that are wrong.", "Small steps still compile.", "Today's forecast: 100% chance of green checkmarks.",
        "Every bug you fix makes a future you smile.", "Take a sip of water. The code will wait.", "Ship it. Or at least commit it.",
        "You asked a computer to write code for you. That is already magic.", "Even the best programmers search for the answer sometimes.", "Breathe. It is only a semicolon.",
        "Your future self says thank you for those comments.", "Plan, build, learn, repeat. You are on step two.", "A short walk fixes at least one bug.",
    )
    val VERBS = listOf("Pondering", "Flibbertigibbeting", "Percolating", "Simmering", "Noodling", "Conjuring", "Marinating", "Reticulating", "Cogitating", "Tinkering", "Wibbling", "Spelunking", "Moonwalking",
        "Brewing", "Doodling", "Fermenting", "Herding bits", "Jiving", "Kneading", "Musing", "Puzzling", "Ruminating", "Shimmying", "Sprouting", "Stewing", "Transmuting", "Unfurling", "Vibing", "Whirring",
        "Zigzagging", "Warming the hearth", "Poking the embers", "Toasting marshmallows", "Stoking the fire", "Counting sparks", "Sweeping the chimney")
    val HELLO = listOf("Hallo", "Hola", "Bonjour", "Ciao", "Olá", "Hej", "Salut", "Moi", "Ahoj", "Namaste", "Konnichiwa", "Merhaba", "Hei", "Servus", "Aloha", "Shalom")
    val EMBER = listOf("Warm in here, isn't it?", "I only eat good ideas.", "Poke me again. I dare you.", "Fire safety tip: save your files.", "I'm not hot, I'm just glowing.", "Hearth, sweet hearth.", "Did somebody say s'mores?", "Crackle crackle.")

    /** A greeting for a special day, or null on an ordinary one. */
    fun holiday(): String? {
        val d = java.time.LocalDate.now()
        return when (d.monthValue to d.dayOfMonth) {
            12 to 24, 12 to 25, 12 to 26 -> "Merry Christmas!"; 12 to 31, 1 to 1 -> "Happy New Year!"; 10 to 31 -> "Boo! Happy Halloween"; 11 to 5 -> "Happy Bonfire Night"
            2 to 14 -> "Happy Valentine's Day"; 4 to 1 -> "Nothing is broken today. Probably."; 5 to 4 -> "May the fourth be with you"; else -> null
        }
    }

    /** Things you can type in a chat instead of a message. Returns what to show, or null when [text] is a normal message. */
    fun egg(text: String): String? = when (text.trim().lowercase()) {
        "/confetti", "/party" -> "Party time!"
        "/ember", "/flame" -> EMBER.random()
        "/coffee" -> "Brewing… this is not a real feature, but you deserve one."
        "/hello" -> HELLO.random() + "!"
        "/42" -> "The answer. Now, what was the question?"
        "/sudo" -> "Nice try."
        else -> null
    }

    fun daily(list: List<String>): String = list[(java.time.LocalDate.now().toEpochDay() % list.size).toInt()]

    /** A short beep: [what] is "send" or "done". */
    fun sound(c: Context, what: String) {
        runCatching {
            val tg = ToneGenerator(AudioManager.STREAM_NOTIFICATION, 55)
            tg.startTone(if (what == "done") ToneGenerator.TONE_PROP_ACK else ToneGenerator.TONE_PROP_BEEP, if (what == "done") 260 else 90)
            android.os.Handler(android.os.Looper.getMainLooper()).postDelayed({ runCatching { tg.release() } }, 600)
        }
    }
}

/** Confetti over everything for two seconds each time [burst] goes up. It does not take any touches. */
@Composable
fun Confetti(burst: Int) {
    val progress = remember { Animatable(0f) }
    var shown by remember { mutableStateOf(false) }
    val pieces = remember(burst) { List(90) { Piece(Random.nextFloat(), Random.nextFloat() * .25f, (Random.nextFloat() - .5f) * .5f, Random.nextFloat() * 2f + 1.4f, Random.nextFloat() * 6.3f, (Random.nextFloat() - .5f) * 14f, Random.nextInt(5)) } }
    LaunchedEffect(burst) {
        if (burst > 0) { shown = true; progress.snapTo(0f); progress.animateTo(1f, tween(2400, easing = LinearEasing)); shown = false }
    }
    if (!shown) return
    val colors = listOf(Color(0xFFF0643C), Color(0xFFD9A441), Color(0xFF6A9BCC), Color(0xFF4CAF7D), Color(0xFFB04D2A))
    Popup(properties = PopupProperties(focusable = false, clippingEnabled = false)) {
        Canvas(Modifier.fillMaxSize()) {
            val t = progress.value
            pieces.forEach { p ->
                val x = (p.x + p.vx * t) * size.width
                val y = (p.y0 - .1f + p.fall * t * t * .55f + t * .35f) * size.height
                val a = (1f - t).coerceIn(0f, 1f)
                val ang = p.spin + p.rot * t
                val w = 9.dp.toPx(); val hgt = 5.dp.toPx()
                withTransform({ rotate(ang * 57.3f, Offset(x, y)) }) { drawRect(colors[p.color].copy(alpha = a), Offset(x - w / 2, y - hgt / 2), Size(w, hgt)) }
            }
        }
    }
}
private data class Piece(val x: Float, val y0: Float, val vx: Float, val fall: Float, val spin: Float, val rot: Float, val color: Int)

/** Ember: the little flame of Hearth. Tap it and it hops, sparks fly and it says something. */
@Composable
fun Ember() {
    val hop = remember { Animatable(0f) }
    var say by remember { mutableStateOf(Toys.EMBER.random()) }
    var sparks by remember { mutableIntStateOf(0) }
    val spark = remember { Animatable(1f) }
    val bits = remember(sparks) { List(10) { Random.nextFloat() * 6.28f to (Random.nextFloat() * .6f + .5f) } }
    LaunchedEffect(sparks) { if (sparks > 0) { hop.snapTo(0f); hop.animateTo(-1f, tween(130)); hop.animateTo(0f, spring(Spring.DampingRatioHighBouncy, Spring.StiffnessMedium)) } }
    LaunchedEffect(sparks) { if (sparks > 0) { spark.snapTo(0f); spark.animateTo(1f, tween(700)) } }
    Card {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(84.dp).clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClickLabel = "Poke Ember") { sparks++; say = Toys.EMBER.random() }, contentAlignment = Alignment.Center) {
                Canvas(Modifier.fillMaxSize()) {
                    val c = center; val r = size.minDimension / 2
                    bits.forEach { (a, s) -> val d = r * (.5f + spark.value * s); val al = (1f - spark.value).coerceIn(0f, 1f); drawCircle(Color(0xFFD9A441).copy(alpha = al), 3.dp.toPx(), Offset(c.x + cos(a) * d, c.y + sin(a) * d - r * .1f)) }
                }
                Box(Modifier.graphicsLayer { translationY = hop.value * 22.dp.toPx(); scaleY = 1f + hop.value * -.08f }) { Spark(56.dp) }
            }
            Spacer(Modifier.width(14.dp))
            Column(Modifier.weight(1f)) { Label("Ember"); Txt(say, T.body) }
        }
    }
}

/** "Claude is working": a flickering flame, and, when the beta words are on, a playful word that changes every few seconds. */
@Composable
fun WorkingIndicator() {
    val c = LocalContext.current
    val playful = Store.betaOn(c, "verbs")
    var i by remember { mutableIntStateOf(Random.nextInt(Toys.VERBS.size)) }
    LaunchedEffect(playful) { while (playful) { delay(2800); i = (i + 1 + Random.nextInt(Toys.VERBS.size - 1)) % Toys.VERBS.size } }
    val tr = androidx.compose.animation.core.rememberInfiniteTransition(label = "flame")
    val sc by tr.animateFloat(.9f, 1.1f, androidx.compose.animation.core.infiniteRepeatable(tween(650, easing = androidx.compose.animation.core.FastOutSlowInEasing), androidx.compose.animation.core.RepeatMode.Reverse), label = "s")
    val al by tr.animateFloat(.7f, 1f, androidx.compose.animation.core.infiniteRepeatable(tween(900), androidx.compose.animation.core.RepeatMode.Reverse), label = "a")
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(vertical = 6.dp)) {
        Box(Modifier.graphicsLayer { if (Fx.motion) { scaleX = sc; scaleY = sc; alpha = al } }) { Spark(22.dp) }
        Spacer(Modifier.width(10.dp))
        androidx.compose.animation.AnimatedContent(if (playful) Toys.VERBS[i] + "…" else "Claude is working…", transitionSpec = { androidx.compose.animation.fadeIn(tween(300)) togetherWith androidx.compose.animation.fadeOut(tween(300)) }, label = "verb") { Txt(it, T.small, muted = true) }
    }
}
