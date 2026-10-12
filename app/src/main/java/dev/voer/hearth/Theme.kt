package dev.voer.hearth

import android.os.Build
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.remember
import androidx.compose.ui.composed
import androidx.compose.ui.graphics.graphicsLayer
import kotlinx.coroutines.delay
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import kotlin.math.roundToInt
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

val Clay = Color(0xFFF0643C)
val Crail = Color(0xFFC15F3C)

private val Light = lightColorScheme(
    primary = Crail, onPrimary = Color(0xFFFBFAF6), background = Color(0xFFF4F1EA), onBackground = Color(0xFF2F2B20),
    surface = Color(0xFFF4F1EA), onSurface = Color(0xFF2F2B20), surfaceVariant = Color(0xFFFBFAF6),
    onSurfaceVariant = Color(0xFF7F7A6D), outline = Color(0xFFE2DCCD), secondary = Clay,
)
private val Dark = darkColorScheme(
    primary = Clay, onPrimary = Color(0xFF1B1A19), background = Color(0xFF1B1A19), onBackground = Color(0xFFF4F2EC),
    surface = Color(0xFF1B1A19), onSurface = Color(0xFFF4F2EC), surfaceVariant = Color(0xFF252422),
    onSurfaceVariant = Color(0xFF9A968C), outline = Color(0xFF34322E), secondary = Clay,
)

/** App-wide colors: Claude's own palette, or the phone's Material You colors when "System" is chosen. */
object ThemePrefs { var system by mutableStateOf<Boolean?>(null) }

/** Looks: frosted glass (see-through panels over a soft glow) and motion. Off = solid panels and no animation, lighter on weak phones. */
object Fx {
    var glass by mutableStateOf(true)
    var motion by mutableStateOf(true)
    private var loaded = false
    fun load(c: android.content.Context) { if (loaded) return; loaded = true; glass = Store.glass(c); motion = Store.motion(c) }
}

/** The panel look. Frosted: a see-through surface with a light edge and a faint highlight at the top. Plain: the solid surface color. */
@Composable
fun Modifier.panel(shape: androidx.compose.ui.graphics.Shape): Modifier {
    val cs = MaterialTheme.colorScheme
    val dark = cs.background.luminance() < .5f
    return if (Fx.glass) {
        val edge = if (dark) Color.White.copy(alpha = .10f) else Color.White.copy(alpha = .75f)
        this.clip(shape)
            .background(androidx.compose.ui.graphics.Brush.verticalGradient(listOf(cs.surfaceVariant.copy(alpha = if (dark) .62f else .78f), cs.surfaceVariant.copy(alpha = if (dark) .44f else .5f))))
            .border(BorderStroke(1.dp, edge), shape)
    } else this.clip(shape).background(cs.surfaceVariant).border(BorderStroke(1.dp, cs.outline), shape)
}

/** The soft colored glow behind everything that makes the glass read as glass. */
@Composable
fun Modifier.glow(): Modifier {
    val cs = MaterialTheme.colorScheme
    if (!Fx.glass) return this.background(cs.background)
    val dark = cs.background.luminance() < .5f
    val a = if (dark) .20f else .24f
    return this.background(cs.background).drawBehind {
        drawRect(androidx.compose.ui.graphics.Brush.radialGradient(listOf(cs.primary.copy(alpha = a), Color.Transparent), Offset(size.width * .05f, size.height * -.02f), size.width * 1.0f))
        drawRect(androidx.compose.ui.graphics.Brush.radialGradient(listOf(Color(0xFF6A9BCC).copy(alpha = a * .8f), Color.Transparent), Offset(size.width * 1.0f, size.height * .15f), size.width * .9f))
        drawRect(androidx.compose.ui.graphics.Brush.radialGradient(listOf(Color(0xFFD9A441).copy(alpha = a * .8f), Color.Transparent), Offset(size.width * .6f, size.height * 1.05f), size.width * 1.0f))
    }
}

@Composable
fun ClaudeTheme(content: @Composable () -> Unit) {
    val c = LocalContext.current
    if (ThemePrefs.system == null) ThemePrefs.system = Store.themeSystem(c)
    Fx.load(c)
    val dark = isSystemInDarkTheme()
    val scheme = when {
        ThemePrefs.system == true && Build.VERSION.SDK_INT >= 31 -> if (dark) dynamicDarkColorScheme(c) else dynamicLightColorScheme(c)
        dark -> Dark
        else -> Light
    }
    MaterialTheme(colorScheme = scheme, content = content)
}

object T {
    val big = TextStyle(fontFamily = FontFamily.Serif, fontSize = 64.sp, letterSpacing = (-1.5).sp)
    val title = TextStyle(fontFamily = FontFamily.Serif, fontSize = 32.sp)
    val h2 = TextStyle(fontFamily = FontFamily.Serif, fontSize = 19.sp)
    val body = TextStyle(fontSize = 15.sp, lineHeight = 21.sp)
    val small = TextStyle(fontSize = 13.sp, lineHeight = 18.sp)
    val label = TextStyle(fontSize = 11.sp, letterSpacing = 1.1.sp, fontWeight = FontWeight.Medium)
}

@Composable
fun Txt(s: String, style: TextStyle = T.body, muted: Boolean = false, color: Color = Color.Unspecified, modifier: Modifier = Modifier) =
    Text(s, modifier, style = style, color = if (color != Color.Unspecified) color else
        if (muted) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface)

@Composable
fun Label(s: String, modifier: Modifier = Modifier) = Txt(s.uppercase(), T.label, muted = true, modifier = modifier)

/** Slides up and fades in the first time it appears. */
fun Modifier.appear(delayMs: Int = 0): Modifier = composed {
    val a = remember { Animatable(if (Fx.motion) 0f else 1f) }
    LaunchedEffect(Unit) { if (Fx.motion) { delay(delayMs.toLong()); a.animateTo(1f, tween(340, easing = FastOutSlowInEasing)) } }
    graphicsLayer { alpha = a.value; translationY = (1f - a.value) * 28f }
}

/** Shrinks a little while pressed and springs back. */
fun Modifier.pressable(onClick: () -> Unit): Modifier = composed {
    val src = remember { MutableInteractionSource() }
    val pressed by src.collectIsPressedAsState()
    val s by animateFloatAsState(if (pressed && Fx.motion) 0.96f else 1f, spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium), label = "press")
    graphicsLayer { scaleX = s; scaleY = s }.clickable(interactionSource = src, indication = null, onClick = onClick)
}

@Composable
fun Card(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    val shape = RoundedCornerShape(20.dp)
    Column(
        modifier.appear().fillMaxWidth().panel(shape).padding(18.dp),
        content = content,
    )
}

@Composable
fun Meter(pct: Float, marker: Float? = null, height: Dp = 8.dp, modifier: Modifier = Modifier) {
    val track = MaterialTheme.colorScheme.outline
    val fill = MaterialTheme.colorScheme.primary
    val mark = MaterialTheme.colorScheme.onSurface
    Canvas(modifier.fillMaxWidth().height(height + 8.dp)) {
        val h = height.toPx(); val y = (size.height - h) / 2
        drawRoundRect(track, Offset(0f, y), Size(size.width, h), CornerRadius(h / 2))
        val w = size.width * (pct / 100f).coerceIn(0f, 1f)
        if (w > 0) drawRoundRect(fill, Offset(0f, y), Size(w.coerceAtLeast(h), h), CornerRadius(h / 2))
        if (marker != null) {
            val x = size.width * marker.coerceIn(0f, 1f)
            drawLine(mark, Offset(x, y - 3.dp.toPx()), Offset(x, y + h + 3.dp.toPx()), 1.5.dp.toPx(), StrokeCap.Round)
        }
    }
}

private val flame = androidx.compose.ui.graphics.vector.PathParser().parsePathString("M256 96c10 52 78 86 78 170a78 78 0 0 1-156 0c0-34 18-58 34-76 2 24 14 38 28 44-8-50 4-98 16-138z").toPath()
private val flameCore = androidx.compose.ui.graphics.vector.PathParser().parsePathString("M256 262c6 26 38 40 38 72a38 38 0 0 1-76 0c0-20 10-30 20-42 2 12 8 18 14 20-4-24 0-38 4-50z").toPath()

@Composable
fun Spark(size: Dp, color: Color = MaterialTheme.colorScheme.primary) {
    val hole = Color(0xFFFFD3A8)
    Canvas(Modifier.size(size)) {
        val k = this.size.width / 320f
        withTransform({ scale(k, k, Offset.Zero); translate(-96f, -80f) }) {
            drawPath(flame, color)
            drawPath(flameCore, hole)
            drawLine(color, Offset(150f, 392f), Offset(362f, 392f), 24f, StrokeCap.Round)
        }
    }
}

@Composable
fun Chip(text: String, selected: Boolean, onClick: () -> Unit) {
    val shape = RoundedCornerShape(50)
    val cs = MaterialTheme.colorScheme
    val group = LocalChipGroup.current
    val line by androidx.compose.animation.animateColorAsState(if (selected) (if (group != null) Color.Transparent else cs.primary) else cs.outline, androidx.compose.animation.core.tween(220), label = "chipline")
    val ink by androidx.compose.animation.animateColorAsState(if (selected) cs.onPrimary else cs.onSurface, androidx.compose.animation.core.tween(220), label = "chipink")
    val solid by androidx.compose.animation.animateColorAsState(if (selected && group == null) cs.primary else Color.Transparent, androidx.compose.animation.core.tween(220), label = "chipbg")
    Box(
        Modifier.pressable(onClick).onGloballyPositioned { c -> if (selected && group != null) group.select(c.boundsInRoot()) }
            .clip(shape).background(solid).border(1.dp, line, shape).padding(horizontal = 14.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) { Txt(text, T.small, color = ink) }
}

/** The selected chip's highlight is one shape that glides to the next chip; the chips themselves never move. */
class ChipGroupState { var origin by mutableStateOf(Offset.Zero); var sel by mutableStateOf<androidx.compose.ui.geometry.Rect?>(null)
    fun select(r: androidx.compose.ui.geometry.Rect) { if (sel != r) sel = r } }
val LocalChipGroup = androidx.compose.runtime.compositionLocalOf<ChipGroupState?> { null }

@Composable
fun ChipRow(modifier: Modifier = Modifier, horizontalArrangement: Arrangement.Horizontal = Arrangement.spacedBy(8.dp), content: @Composable RowScope.() -> Unit) {
    val st = remember { ChipGroupState() }
    val primary = MaterialTheme.colorScheme.primary
    androidx.compose.runtime.CompositionLocalProvider(LocalChipGroup provides st) {
        Box(modifier.onGloballyPositioned { st.origin = it.boundsInRoot().topLeft }) {
            val r = st.sel
            if (r != null) {
                val spec = androidx.compose.animation.core.tween<Float>(380, easing = androidx.compose.animation.core.FastOutSlowInEasing)
                val x by animateFloatAsState(r.left - st.origin.x, spec, label = "hlx"); val y by animateFloatAsState(r.top - st.origin.y, spec, label = "hly")
                val w by animateFloatAsState(r.width, spec, label = "hlw"); val hh by animateFloatAsState(r.height, spec, label = "hlh")
                Box(Modifier.offset { androidx.compose.ui.unit.IntOffset(x.roundToInt(), y.roundToInt()) }.size(with(LocalDensity.current) { w.toDp() }, with(LocalDensity.current) { hh.toDp() }).clip(RoundedCornerShape(50)).background(primary))
            }
            Row(horizontalArrangement = horizontalArrangement, content = content)
        }
    }
}

@Composable
fun Button(text: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val cs = MaterialTheme.colorScheme
    Box(
        modifier.pressable(onClick).clip(RoundedCornerShape(14.dp)).background(cs.primary).padding(horizontal = 20.dp, vertical = 14.dp),
        contentAlignment = Alignment.Center,
    ) { Txt(text, T.body.copy(fontWeight = FontWeight.Medium), color = cs.onPrimary) }
}

@Composable
fun TextAction(text: String, onClick: () -> Unit) =
    Txt(text, T.small.copy(fontWeight = FontWeight.Medium), color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.clickable(onClick = onClick).padding(vertical = 6.dp))
