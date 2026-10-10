package dev.voer.hearth

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.runtime.snapshots.SnapshotStateMap
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

/** The guided tour: dims the screen, lights up one thing at a time and explains it. */
object Tour {
    var active by mutableStateOf(false)
    var step by mutableIntStateOf(0)
    val bounds: SnapshotStateMap<String, Rect> = mutableStateMapOf()
    fun start() { step = 0; active = true; Ui.tab = 0 }
}
/** Marks a part of the screen the tour may point at. */
fun Modifier.tourTarget(key: String): Modifier = onGloballyPositioned { Tour.bounds[key] = it.boundsInRoot() }

private data class TourStep(val key: String?, val title: String, val text: String, val tab: Int = 0)
private val STEPS = listOf(
    TourStep(null, "This is Hearth", "A quick look around. I will point at things and say what they do."),
    TourStep("tabs", "The menu", "Sessions (your chats), Usage (how much of your limit is left and what it costs) and Settings."),
    TourStep("new", "Start a chat", "Tap here to start a new session. Pick a project, choose which computer works on it, and say what you want."),
    TourStep("list", "Your chats", "These are the same chats as on your laptop and your server. Tap one to continue it."),
    TourStep("usage", "Usage", "Your 5-hour and weekly limits, with history.", 1),
    TourStep("computers", "Computers and servers", "Add or remove the computers and servers this phone talks to. Tap “Add a computer”, then Connect.", 2),
    TourStep("updates", "Updates", "Hearth updates itself from GitHub. Press Check for updates; it can update the server too.", 2),
    TourStep(null, "That is the tour", "Need help? Settings → Support. Have fun!", 0),
)

@Composable
fun TourOverlay() {
    if (!Tour.active) return
    val s = STEPS[Tour.step.coerceIn(0, STEPS.lastIndex)]
    LaunchedEffect(Tour.step) { Ui.tab = s.tab }
    val screenH = with(LocalDensity.current) { LocalConfiguration.current.screenHeightDp.dp.toPx() }
    val screenW = with(LocalDensity.current) { LocalConfiguration.current.screenWidthDp.dp.toPx() }
    val target = s.key?.let { Tour.bounds[it] }?.takeIf { it.width > 0 && it.bottom > 0 && it.top < screenH }
    val pad = with(LocalDensity.current) { 8.dp.toPx() }
    val hole = target?.let { Rect(it.left - pad, it.top - pad, it.right + pad, it.bottom + pad) }
    // the light glides from one thing to the next
    val l = remember { Animatable(0f) }; val t = remember { Animatable(0f) }; val r = remember { Animatable(0f) }; val b = remember { Animatable(0f) }; val a = remember { Animatable(0f) }
    LaunchedEffect(hole) {
        if (hole == null) a.animateTo(0f, tween(250)) else {
            val first = a.value == 0f
            if (first) { l.snapTo(hole.left); t.snapTo(hole.top); r.snapTo(hole.right); b.snapTo(hole.bottom) }
            a.animateTo(1f, tween(300))
            if (!first) { l.animateTo(hole.left, tween(420)); t.animateTo(hole.top, tween(420)); r.animateTo(hole.right, tween(420)); b.animateTo(hole.bottom, tween(420)) }
        }
    }
    val primary = MaterialTheme.colorScheme.primary
    Box(Modifier.fillMaxSize().clickable(enabled = true, indication = null, interactionSource = remember { androidx.compose.foundation.interaction.MutableInteractionSource() }) {}) {
        Canvas(Modifier.fillMaxSize().graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }) {
            drawRect(Color.Black.copy(alpha = 0.72f))
            if (a.value > 0f) {
                val rc = Rect(l.value, t.value, r.value, b.value)
                drawRoundRect(Color.Transparent, rc.topLeft, rc.size, CornerRadius(28f), blendMode = BlendMode.Clear)
                drawRoundRect(primary.copy(alpha = a.value), rc.topLeft, rc.size, CornerRadius(28f), style = androidx.compose.ui.graphics.drawscope.Stroke(width = 4f))
            }
        }
        val below = hole == null || hole.center.y < screenH / 2
        AnimatedContent(Tour.step, Modifier.align(if (hole == null) Alignment.Center else if (below) Alignment.BottomCenter else Alignment.TopCenter).padding(20.dp).navigationBarsPadding().statusBarsPadding(), transitionSpec = {
            (slideInVertically(tween(320)) { if (below) it / 4 else -it / 4 } + fadeIn(tween(320))) togetherWith (slideOutVertically(tween(200)) { if (below) it / 4 else -it / 4 } + fadeOut(tween(160)))
        }, label = "tourcard") { n ->
            val st = STEPS[n.coerceIn(0, STEPS.lastIndex)]
            Column(Modifier.fillMaxWidth().clip(RoundedCornerShape(18.dp)).background(MaterialTheme.colorScheme.surfaceVariant).padding(18.dp)) {
                Txt("${n + 1} / ${STEPS.size}", T.small, muted = true)
                Spacer(Modifier.height(2.dp))
                Txt(st.title, T.h2)
                Spacer(Modifier.height(6.dp))
                Txt(st.text, T.body, muted = true)
                Spacer(Modifier.height(14.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    TextAction("Skip tour") { Tour.active = false }
                    Spacer(Modifier.weight(1f))
                    if (n > 0) { TextAction("Back") { Tour.step = n - 1 }; Spacer(Modifier.width(16.dp)) }
                    Button(if (n == STEPS.lastIndex) "Finish" else "Next", Modifier) { if (n == STEPS.lastIndex) Tour.active = false else Tour.step = n + 1 }
                }
            }
        }
    }
}
