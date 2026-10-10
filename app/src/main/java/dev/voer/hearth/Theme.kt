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
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.draw.clip
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
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

val Clay = Color(0xFFF0643C)
val Crail = Color(0xFFC15F3C)

private val Light = lightColorScheme(
    primary = Crail, onPrimary = Color(0xFFFAF9F5), background = Color(0xFFF0EEE6), onBackground = Color(0xFF3D3929),
    surface = Color(0xFFF0EEE6), onSurface = Color(0xFF3D3929), surfaceVariant = Color(0xFFFAF9F5),
    onSurfaceVariant = Color(0xFF83827D), outline = Color(0xFFDDD9CB), secondary = Clay,
)
private val Dark = darkColorScheme(
    primary = Clay, onPrimary = Color(0xFF1F1E1D), background = Color(0xFF1F1E1D), onBackground = Color(0xFFFAF9F5),
    surface = Color(0xFF1F1E1D), onSurface = Color(0xFFFAF9F5), surfaceVariant = Color(0xFF2B2A28),
    onSurfaceVariant = Color(0xFF9C9A92), outline = Color(0xFF3F3E3A), secondary = Clay,
)

/** App-wide colors: Claude's own palette, or the phone's Material You colors when "System" is chosen. */
object ThemePrefs { var system by mutableStateOf<Boolean?>(null) }

@Composable
fun ClaudeTheme(content: @Composable () -> Unit) {
    val c = LocalContext.current
    if (ThemePrefs.system == null) ThemePrefs.system = Store.themeSystem(c)
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
    val title = TextStyle(fontFamily = FontFamily.Serif, fontSize = 26.sp)
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
    val a = remember { Animatable(0f) }
    LaunchedEffect(Unit) { delay(delayMs.toLong()); a.animateTo(1f, tween(340, easing = FastOutSlowInEasing)) }
    graphicsLayer { alpha = a.value; translationY = (1f - a.value) * 28f }
}

/** Shrinks a little while pressed and springs back. */
fun Modifier.pressable(onClick: () -> Unit): Modifier = composed {
    val src = remember { MutableInteractionSource() }
    val pressed by src.collectIsPressedAsState()
    val s by animateFloatAsState(if (pressed) 0.96f else 1f, spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium), label = "press")
    graphicsLayer { scaleX = s; scaleY = s }.clickable(interactionSource = src, indication = null, onClick = onClick)
}

@Composable
fun Card(modifier: Modifier = Modifier, content: @Composable ColumnScope.() -> Unit) {
    val shape = RoundedCornerShape(16.dp)
    Column(
        modifier.appear().fillMaxWidth().clip(shape).background(MaterialTheme.colorScheme.surfaceVariant)
            .border(BorderStroke(1.dp, MaterialTheme.colorScheme.outline), shape).padding(18.dp),
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
    // the highlight fades and the label recolors smoothly when the selection moves
    val bg by androidx.compose.animation.animateColorAsState(if (selected) cs.primary else Color.Transparent, androidx.compose.animation.core.tween(220), label = "chipbg")
    val line by androidx.compose.animation.animateColorAsState(if (selected) cs.primary else cs.outline, androidx.compose.animation.core.tween(220), label = "chipline")
    val ink by androidx.compose.animation.animateColorAsState(if (selected) cs.onPrimary else cs.onSurface, androidx.compose.animation.core.tween(220), label = "chipink")
    Box(
        Modifier.pressable(onClick).clip(shape).background(bg).border(1.dp, line, shape).padding(horizontal = 14.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) { Txt(text, T.small, color = ink) }
}

@Composable
fun Button(text: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val cs = MaterialTheme.colorScheme
    Box(
        modifier.pressable(onClick).clip(RoundedCornerShape(12.dp)).background(cs.primary).padding(horizontal = 20.dp, vertical = 14.dp),
        contentAlignment = Alignment.Center,
    ) { Txt(text, T.body.copy(fontWeight = FontWeight.Medium), color = cs.onPrimary) }
}

@Composable
fun TextAction(text: String, onClick: () -> Unit) =
    Txt(text, T.small.copy(fontWeight = FontWeight.Medium), color = MaterialTheme.colorScheme.primary,
        modifier = Modifier.clickable(onClick = onClick).padding(vertical = 6.dp))
