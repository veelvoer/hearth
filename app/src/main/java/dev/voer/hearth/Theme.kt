package dev.voer.hearth

import android.os.Build
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.BorderStroke
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.togetherWith
import androidx.compose.animation.core.animateFloat
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.*
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.role
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
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.VectorConverter
import androidx.compose.runtime.rememberCoroutineScope
import kotlinx.coroutines.launch
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
import androidx.compose.ui.layout.positionInParent
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import kotlin.math.roundToInt
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sin

val Clay = Color(0xFFF0643C)
val Crail = Color(0xFFB04D2A)   // darker than before: text and buttons in this color now reach 4.5:1 contrast on the light background

private val Light = lightColorScheme(
    primary = Crail, onPrimary = Color(0xFFFBFAF6), background = Color(0xFFF4F1EA), onBackground = Color(0xFF2F2B20),
    surface = Color(0xFFF4F1EA), onSurface = Color(0xFF2F2B20), surfaceVariant = Color(0xFFFBFAF6),
    onSurfaceVariant = Color(0xFF6B665A), outline = Color(0xFFE2DCCD), outlineVariant = Color(0xFF8F8A7B), secondary = Clay,   // outlineVariant: the border of inputs and chips (3:1 against the surface)
)
private val Dark = darkColorScheme(
    primary = Clay, onPrimary = Color(0xFF1B1A19), background = Color(0xFF1B1A19), onBackground = Color(0xFFF4F2EC),
    surface = Color(0xFF1B1A19), onSurface = Color(0xFFF4F2EC), surfaceVariant = Color(0xFF252422),
    onSurfaceVariant = Color(0xFF9A968C), outline = Color(0xFF34322E), outlineVariant = Color(0xFF77736A), secondary = Clay,
)

/** App-wide colors: Claude's own palette, or the phone's Material You colors when "System" is chosen. */
object ThemePrefs { var system by mutableStateOf<Boolean?>(null) }

/** Looks: frosted glass (see-through panels over a soft glow) and motion. Off = solid panels and no animation, lighter on weak phones. */
object Fx {
    var glass by mutableStateOf(true)
    var motion by mutableStateOf(true)
    /** Material 3: Google's design language (tonal surfaces, round buttons, a standard navigation bar). Off = the Hearth look. */
    var m3 by mutableStateOf(false)
    private var loaded = false
    fun load(c: android.content.Context) { if (loaded) return; loaded = true; glass = Store.glass(c); motion = Store.motion(c); m3 = Store.m3(c) }
}

@Composable
fun systemColors(): Boolean = ThemePrefs.system == true && Build.VERSION.SDK_INT >= 31

/** The quiet fill behind avatars, your message bubbles and small tags. */
@Composable
fun softFill(): Color {
    val cs = MaterialTheme.colorScheme
    return if (Fx.m3 || systemColors()) cs.secondaryContainer else if (cs.background.luminance() < .5f) Color(0xFF2E2A26) else Color(0xFFE9E2D2)
}

/** The panel look. Frosted: a see-through surface with a light edge and a faint highlight at the top. Plain: the solid surface color. */
@Composable
fun Modifier.panel(shape: androidx.compose.ui.graphics.Shape): Modifier {
    val cs = MaterialTheme.colorScheme
    if (Fx.m3) return this.clip(shape).background(cs.surfaceContainer)   // Material 3: a filled card, no border
    val dark = cs.background.luminance() < .5f
    return if (Fx.glass && !systemColors()) {
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
    if (Fx.m3 || !Fx.glass || systemColors()) return this.background(cs.background)
    val dark = cs.background.luminance() < .5f
    val a = if (dark) .20f else .24f
    return this.background(cs.background).drawBehind {
        drawRect(androidx.compose.ui.graphics.Brush.radialGradient(listOf(cs.primary.copy(alpha = a), Color.Transparent), Offset(size.width * .05f, size.height * -.02f), size.width * 1.0f))
        drawRect(androidx.compose.ui.graphics.Brush.radialGradient(listOf(Color(0xFF6A9BCC).copy(alpha = a * .8f), Color.Transparent), Offset(size.width * 1.0f, size.height * .15f), size.width * .9f))
        drawRect(androidx.compose.ui.graphics.Brush.radialGradient(listOf(Color(0xFFD9A441).copy(alpha = a * .8f), Color.Transparent), Offset(size.width * .6f, size.height * 1.05f), size.width * 1.0f))
    }
}

/** The Hearth colors as a full Material 3 scheme (made from the Crail seed the way Material You makes one), for Material 3 style without the wallpaper colors. */
private val M3Light = lightColorScheme(
    primary = Color(0xFF9A4122), onPrimary = Color(0xFFFFFFFF), primaryContainer = Color(0xFFFFDBCF), onPrimaryContainer = Color(0xFF380D00),
    secondary = Color(0xFF77574D), onSecondary = Color(0xFFFFFFFF), secondaryContainer = Color(0xFFF5DDD5), onSecondaryContainer = Color(0xFF2C150F),
    tertiary = Color(0xFF6B5E2F), onTertiary = Color(0xFFFFFFFF), tertiaryContainer = Color(0xFFF4E2A7), onTertiaryContainer = Color(0xFF231B00),
    error = Color(0xFFBA1A1A), onError = Color(0xFFFFFFFF), errorContainer = Color(0xFFFFDAD6), onErrorContainer = Color(0xFF410002),
    background = Color(0xFFFFF8F6), onBackground = Color(0xFF231917), surface = Color(0xFFFFF8F6), onSurface = Color(0xFF231917),
    surfaceVariant = Color(0xFFF5DED6), onSurfaceVariant = Color(0xFF53433E), outline = Color(0xFF85736D), outlineVariant = Color(0xFFD8C2BB),
    inverseSurface = Color(0xFF392E2B), inverseOnSurface = Color(0xFFFFEDE8), inversePrimary = Color(0xFFFFB59D), surfaceTint = Color(0xFF9A4122),
    surfaceDim = Color(0xFFE8D6D2), surfaceBright = Color(0xFFFFF8F6), surfaceContainerLowest = Color(0xFFFFFFFF), surfaceContainerLow = Color(0xFFFFF1ED),
    surfaceContainer = Color(0xFFFCEAE5), surfaceContainerHigh = Color(0xFFF6E4DF), surfaceContainerHighest = Color(0xFFF0DEDA),
)
private val M3Dark = darkColorScheme(
    primary = Color(0xFFFFB59D), onPrimary = Color(0xFF5B1A00), primaryContainer = Color(0xFF7C2E0F), onPrimaryContainer = Color(0xFFFFDBCF),
    secondary = Color(0xFFE7BDB0), onSecondary = Color(0xFF442A21), secondaryContainer = Color(0xFF5D4036), onSecondaryContainer = Color(0xFFFFDBCF),
    tertiary = Color(0xFFD7C58D), onTertiary = Color(0xFF3B2F05), tertiaryContainer = Color(0xFF53461A), onTertiaryContainer = Color(0xFFF4E2A7),
    error = Color(0xFFFFB4AB), onError = Color(0xFF690005), errorContainer = Color(0xFF93000A), onErrorContainer = Color(0xFFFFDAD6),
    background = Color(0xFF1A110F), onBackground = Color(0xFFF1DFDA), surface = Color(0xFF1A110F), onSurface = Color(0xFFF1DFDA),
    surfaceVariant = Color(0xFF53433E), onSurfaceVariant = Color(0xFFD8C2BB), outline = Color(0xFFA08D86), outlineVariant = Color(0xFF53433E),
    inverseSurface = Color(0xFFF1DFDA), inverseOnSurface = Color(0xFF392E2B), inversePrimary = Color(0xFF9A4122), surfaceTint = Color(0xFFFFB59D),
    surfaceDim = Color(0xFF1A110F), surfaceBright = Color(0xFF423734), surfaceContainerLowest = Color(0xFF140C0A), surfaceContainerLow = Color(0xFF231917),
    surfaceContainer = Color(0xFF271D1B), surfaceContainerHigh = Color(0xFF322825), surfaceContainerHighest = Color(0xFF3D3230),
)
/** The app reads its own roles from a scheme (a card is surfaceVariant, a hairline is outline, a stronger line is outlineVariant): a Material 3 scheme is arranged the same way. */
private fun androidx.compose.material3.ColorScheme.forApp() = copy(background = surface, surfaceVariant = surfaceContainer, outline = outlineVariant, outlineVariant = outline)

private val M3Shapes = Shapes(extraSmall = RoundedCornerShape(4.dp), small = RoundedCornerShape(8.dp), medium = RoundedCornerShape(12.dp), large = RoundedCornerShape(16.dp), extraLarge = RoundedCornerShape(28.dp))

@Composable
fun ClaudeTheme(content: @Composable () -> Unit) {
    val c = LocalContext.current
    if (ThemePrefs.system == null) ThemePrefs.system = Store.themeSystem(c)
    Fx.load(c)
    val dark = isSystemInDarkTheme()
    val scheme = when {
        Fx.m3 && ThemePrefs.system == true && Build.VERSION.SDK_INT >= 31 -> (if (dark) dynamicDarkColorScheme(c) else dynamicLightColorScheme(c)).forApp()
        Fx.m3 -> (if (dark) M3Dark else M3Light).forApp()
        ThemePrefs.system == true && Build.VERSION.SDK_INT >= 31 -> (if (dark) dynamicDarkColorScheme(c) else dynamicLightColorScheme(c)).let { d ->
            // the phone's palette, arranged the way the app uses it: quiet cards on a plain page, a light hairline, a stronger line for inputs
            d.copy(background = d.surface, surfaceVariant = d.surfaceContainerLow, outline = d.outlineVariant, outlineVariant = d.outline)
        }
        dark -> Dark
        else -> Light
    }
    if (Fx.m3) MaterialTheme(colorScheme = scheme, shapes = M3Shapes, content = content) else MaterialTheme(colorScheme = scheme, content = content)
}

object T {
    // Material 3 uses the system font and its type scale; the Hearth look uses a serif for titles
    val big get() = if (Fx.m3) TextStyle(fontSize = 57.sp, lineHeight = 64.sp, letterSpacing = (-0.25).sp) else TextStyle(fontFamily = FontFamily.Serif, fontSize = 64.sp, letterSpacing = (-1.5).sp)
    val title get() = if (Fx.m3) TextStyle(fontSize = 32.sp, lineHeight = 40.sp) else TextStyle(fontFamily = FontFamily.Serif, fontSize = 32.sp)
    val h2 get() = if (Fx.m3) TextStyle(fontSize = 22.sp, lineHeight = 28.sp) else TextStyle(fontFamily = FontFamily.Serif, fontSize = 20.sp)
    val body get() = if (Fx.m3) TextStyle(fontSize = 16.sp, lineHeight = 24.sp, letterSpacing = 0.5.sp) else TextStyle(fontSize = 16.sp, lineHeight = 23.sp)
    val small get() = if (Fx.m3) TextStyle(fontSize = 14.sp, lineHeight = 20.sp, letterSpacing = 0.25.sp) else TextStyle(fontSize = 14.sp, lineHeight = 20.sp)
    val label get() = if (Fx.m3) TextStyle(fontSize = 12.sp, lineHeight = 16.sp, letterSpacing = 0.5.sp, fontWeight = FontWeight.Medium) else TextStyle(fontSize = 12.sp, letterSpacing = 1.1.sp, fontWeight = FontWeight.Medium)
}

@Composable
fun Txt(s: String, style: TextStyle = T.body, muted: Boolean = false, color: Color = Color.Unspecified, modifier: Modifier = Modifier, maxLines: Int = Int.MAX_VALUE) =
    Text(s, modifier, maxLines = if (LocalCalm.current && muted && s.length > 60 && maxLines == Int.MAX_VALUE) 1 else maxLines, overflow = if (maxLines == Int.MAX_VALUE && !(LocalCalm.current && muted && s.length > 60)) androidx.compose.ui.text.style.TextOverflow.Clip else androidx.compose.ui.text.style.TextOverflow.Ellipsis, style = style, color = if (color != Color.Unspecified) color else
        if (muted) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface)

@Composable
fun Label(s: String, modifier: Modifier = Modifier) = Txt(if (Fx.m3) s else s.uppercase(), T.label, muted = true, modifier = modifier)

/** Slides up and fades in the first time it appears. */
/** Inside Settings nothing slides, bounces or fades in: pages should just be there. */
val LocalCalm = androidx.compose.runtime.compositionLocalOf { false }

fun Modifier.appear(delayMs: Int = 0): Modifier = composed {
    if (LocalCalm.current) return@composed this
    if (Fx.m3) return@composed this.springIn()
    val a = remember { Animatable(if (Fx.motion) 0f else 1f) }
    LaunchedEffect(Unit) { if (Fx.motion) { delay(delayMs.toLong()); a.animateTo(1f, tween(340, easing = FastOutSlowInEasing)) } }
    graphicsLayer { alpha = a.value; translationY = (1f - a.value) * 28f }
}

/** Shrinks a little while pressed and springs back. */
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
fun Modifier.pressable(onLong: (() -> Unit)? = null, shape: androidx.compose.ui.graphics.Shape? = null, onClick: () -> Unit): Modifier = composed {
    // Material 3 with motion on: the expressive press (leans toward the finger, a light follows it, springs back)
    if (Fx.m3 && Fx.motion && !LocalCalm.current) return@composed this.elastic(shape ?: RoundedCornerShape(16.dp), onLong = onLong, onClick = onClick)
    val hap = androidx.compose.ui.platform.LocalHapticFeedback.current
    val src = remember { MutableInteractionSource() }
    val pressed by src.collectIsPressedAsState()
    val calm = LocalCalm.current
    val m3 = Fx.m3
    val ind = if (m3) androidx.compose.material3.ripple() else null   // Material 3 answers a touch with a ripple, not with a shrinking card
    val s by animateFloatAsState(if (pressed && Fx.motion && !calm && !m3) 0.96f else 1f, spring(dampingRatio = Spring.DampingRatioMediumBouncy, stiffness = Spring.StiffnessMedium), label = "press")
    graphicsLayer { scaleX = s; scaleY = s }.let { m -> if (onLong == null) m.clickable(interactionSource = src, indication = ind, onClick = { hap.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.TextHandleMove); onClick() }) else m.combinedClickable(interactionSource = src, indication = ind, onClick = { hap.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.TextHandleMove); onClick() }, onLongClick = { hap.performHapticFeedback(androidx.compose.ui.hapticfeedback.HapticFeedbackType.LongPress); onLong() }) }
}

@Composable
fun Card(modifier: Modifier = Modifier, flat: Boolean = false, content: @Composable ColumnScope.() -> Unit) {
    val shape = RoundedCornerShape(if (Fx.m3) 16.dp else 20.dp)
    if (flat) { Column(modifier.fillMaxWidth(), content = content); return }
    Column(
        modifier.appear().fillMaxWidth().panel(shape).animateContentSize().padding(start = 18.dp, end = 18.dp, top = 16.dp, bottom = 12.dp),
        content = content,
    )
}

/** A title that groups the cards under it, so a long page is easy to find your way in. */
@Composable
fun SectionTitle(text: String, icon: String? = null) { Row(Modifier.padding(top = 14.dp, start = 4.dp), verticalAlignment = Alignment.CenterVertically) { if (icon != null) { Glyph(icon, 20.dp, MaterialTheme.colorScheme.primary); Spacer(Modifier.width(8.dp)) }; Txt(text, T.h2.copy(fontSize = 20.sp), color = MaterialTheme.colorScheme.primary) } }

/** Folding cards start open on pages that hold only a few of them. */
val LocalFoldOpen = androidx.compose.runtime.compositionLocalOf { false }

/** A card that is closed until you tap its title, so a long page of settings reads as a short list. The (i) keeps the explanation. */
@Composable
fun Fold(icon: String, title: String, tip: String? = null, content: @Composable ColumnScope.() -> Unit) {
    val startOpen = LocalFoldOpen.current
    var open by remember { mutableStateOf(startOpen) }
    val shape = RoundedCornerShape(if (Fx.m3) 16.dp else 20.dp)
    Column(Modifier.appear().fillMaxWidth().panel(shape).padding(horizontal = 18.dp, vertical = 6.dp)) {
        Row(Modifier.fillMaxWidth().clickable { open = !open }.heightIn(min = 52.dp), verticalAlignment = Alignment.CenterVertically) {
            Glyph(icon, 18.dp); Spacer(Modifier.width(8.dp)); Label(title, Modifier.weight(1f))
            if (tip != null) androidx.compose.runtime.CompositionLocalProvider(LocalIconAlign provides Alignment.CenterEnd) { InfoDot(title, tip) }
            val rot by androidx.compose.animation.core.animateFloatAsState(if (open) 180f else 0f, if (Fx.m3) Spr.fast() else androidx.compose.animation.core.tween(if (Fx.motion) 280 else 0, easing = androidx.compose.animation.core.FastOutSlowInEasing), label = "chev")
            Box(Modifier.size(48.dp), contentAlignment = Alignment.CenterEnd) { Box(Modifier.graphicsLayer { rotationZ = rot }) { Glyph("chevd", 22.dp) } }
        }
        run {
            if (Fx.m3 && Fx.motion) androidx.compose.animation.AnimatedVisibility(open, enter = androidx.compose.animation.expandVertically(Spr.move()) + androidx.compose.animation.fadeIn(Spr.fade()) + androidx.compose.animation.scaleIn(Spr.move(), initialScale = .96f, transformOrigin = androidx.compose.ui.graphics.TransformOrigin(0.5f, 0f)), exit = androidx.compose.animation.shrinkVertically(Spr.move()) + androidx.compose.animation.fadeOut(Spr.fade())) { Column(Modifier.padding(bottom = 12.dp)) { content() } }
            else androidx.compose.animation.AnimatedVisibility(open, enter = androidx.compose.animation.expandVertically(androidx.compose.animation.core.tween(if (Fx.motion) 280 else 0, easing = androidx.compose.animation.core.FastOutSlowInEasing)) + androidx.compose.animation.fadeIn(androidx.compose.animation.core.tween(if (Fx.motion) 220 else 0)), exit = androidx.compose.animation.shrinkVertically(androidx.compose.animation.core.tween(if (Fx.motion) 220 else 0)) + androidx.compose.animation.fadeOut(androidx.compose.animation.core.tween(if (Fx.motion) 140 else 0))) { Column(Modifier.padding(bottom = 12.dp)) { content() } }
        }
    }
}

@Composable
fun Meter(pct: Float, marker: Float? = null, height: Dp = 8.dp, modifier: Modifier = Modifier) {
    val track = if (Fx.m3) MaterialTheme.colorScheme.surfaceContainerHighest else MaterialTheme.colorScheme.outline
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
fun Chip(text: String, selected: Boolean, icon: String? = null, onClick: () -> Unit) {
    val cs = MaterialTheme.colorScheme
    if (Fx.m3) {   // Material 3 filter chip: 8dp corners that round into a pill when chosen, a check when chosen, a tonal fill
        val corner by animateDpAsState(if (selected) 18.dp else 8.dp, Spr.fast(), label = "chipShape")
        val pop = remember { Animatable(1f) }
        LaunchedEffect(selected) { if (selected && Fx.motion) { pop.snapTo(.9f); pop.animateTo(1f, spring(dampingRatio = 0.4f, stiffness = 700f)) } }
        FilterChip(selected = selected, onClick = onClick, label = { Text(text, style = T.small.copy(fontWeight = FontWeight.Medium), maxLines = 1) },
            leadingIcon = if (selected) { { Glyph("check", 18.dp, cs.onSecondaryContainer) } } else if (icon != null) { { Glyph(icon, 18.dp, cs.onSurfaceVariant) } } else null,
            shape = RoundedCornerShape(corner), border = if (selected) null else BorderStroke(1.dp, cs.outlineVariant),
            colors = FilterChipDefaults.filterChipColors(selectedContainerColor = cs.secondaryContainer, selectedLabelColor = cs.onSecondaryContainer, labelColor = cs.onSurfaceVariant, containerColor = Color.Transparent),
            modifier = Modifier.graphicsLayer { scaleX = pop.value; scaleY = pop.value }.semantics { if (icon != null) contentDescription = text })
        return
    }
    val shape = RoundedCornerShape(50)
    val group = LocalChipGroup.current
    val line by androidx.compose.animation.animateColorAsState(if (selected) (if (group != null) Color.Transparent else cs.primary) else cs.outlineVariant, androidx.compose.animation.core.tween(220), label = "chipline")
    val ink by androidx.compose.animation.animateColorAsState(if (selected) cs.onPrimary else cs.onSurface, androidx.compose.animation.core.tween(220), label = "chipink")
    val solid by androidx.compose.animation.animateColorAsState(if (selected && group == null) cs.primary else Color.Transparent, androidx.compose.animation.core.tween(220), label = "chipbg")
    Box(
        Modifier.pressable(onClick = onClick).onGloballyPositioned { c -> if (selected && group != null) group.select(c, c.size.width.toFloat(), c.size.height.toFloat()) }
            .heightIn(min = 48.dp).semantics(mergeDescendants = true) { role = androidx.compose.ui.semantics.Role.Button; this.selected = selected; if (icon != null) contentDescription = text }
            .clip(shape).background(solid).border(1.dp, line, shape).padding(horizontal = 14.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) { Row(verticalAlignment = Alignment.CenterVertically) { if (icon != null) { Glyph(icon, if (selected) 15.dp else 20.dp, ink); if (selected) Spacer(Modifier.width(6.dp)) }; if (icon == null || selected) Txt(text, T.small, color = ink) } }
}

/** The selected chip's highlight is one shape that glides to the next chip; the chips themselves never move. */
class ChipGroupState { var coords: androidx.compose.ui.layout.LayoutCoordinates? = null; var sel by mutableStateOf<androidx.compose.ui.geometry.Rect?>(null)
    /** Where the chip is inside the row's own content, so scrolling the row never moves the highlight on its own. */
    fun select(chip: androidx.compose.ui.layout.LayoutCoordinates, w: Float, h: Float) {
        val o = coords
        val at = if (o != null && o.isAttached && chip.isAttached) o.localPositionOf(chip, Offset.Zero) else chip.positionInParent()
        val r = androidx.compose.ui.geometry.Rect(at.x, at.y, at.x + w, at.y + h)
        if (sel != r) sel = r
    } }
val LocalChipGroup = androidx.compose.runtime.compositionLocalOf<ChipGroupState?> { null }

@Composable
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
fun ChipRow(modifier: Modifier = Modifier, horizontalArrangement: Arrangement.Horizontal = Arrangement.spacedBy(8.dp), scroll: Boolean = false, multi: Boolean = false, content: @Composable () -> Unit) {
    val st = remember { ChipGroupState() }
    val primary = MaterialTheme.colorScheme.primary
    androidx.compose.runtime.CompositionLocalProvider(LocalChipGroup provides (if (multi || Fx.m3) null else st)) {
        Box(modifier) { Box(Modifier.onGloballyPositioned { st.coords = it }) {
            val r = st.sel
            if (r != null && !Fx.m3) {
                val spec = androidx.compose.animation.core.tween<Float>(380, easing = androidx.compose.animation.core.FastOutSlowInEasing)
                val x by animateFloatAsState(r.left, spec, label = "hlx"); val y by animateFloatAsState(r.top, spec, label = "hly")
                val w by animateFloatAsState(r.width, spec, label = "hlw"); val hh by animateFloatAsState(r.height, spec, label = "hlh")
                Box(Modifier.offset { androidx.compose.ui.unit.IntOffset(x.roundToInt(), y.roundToInt()) }.size(with(LocalDensity.current) { w.toDp() }, with(LocalDensity.current) { hh.toDp() }).clip(RoundedCornerShape(50)).background(primary))
            }
            if (scroll) Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = horizontalArrangement) { content() }
            else FlowRow(horizontalArrangement = horizontalArrangement, verticalArrangement = Arrangement.spacedBy(8.dp)) { content() }
        } }
    }
}

@Composable
fun Button(text: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    val cs = MaterialTheme.colorScheme
    if (Fx.m3) {   // the Material 3 filled button: a pill that squares up a little while pressed and springs back (the expressive shape morph)
        val src = remember { MutableInteractionSource() }
        val pressed by src.collectIsPressedAsState()
        val corner by animateFloatAsState(if (pressed && Fx.motion) 22f else 50f, Spr.fast(), label = "btnShape")
        val squash by animateFloatAsState(if (pressed && Fx.motion) .96f else 1f, Spr.fast(), label = "btnSquash")
        androidx.compose.material3.Button(onClick = onClick, interactionSource = src, modifier = modifier.graphicsLayer { scaleX = squash; scaleY = squash }, shape = RoundedCornerShape(corner.coerceIn(0f, 50f).toInt()), contentPadding = PaddingValues(horizontal = 24.dp, vertical = 10.dp)) {
            val ic = ACTION_ICONS[text]
            if (ic != null) { Glyph(ic, 18.dp, cs.onPrimary); Spacer(Modifier.width(8.dp)) }
            Text(text, style = T.small.copy(fontWeight = FontWeight.Medium, letterSpacing = 0.1.sp), maxLines = 1)
        }
        return
    }
    Box(
        modifier.pressable(onClick = onClick).clip(RoundedCornerShape(14.dp)).background(cs.primary).padding(horizontal = 20.dp, vertical = 14.dp),
        contentAlignment = Alignment.Center,
    ) { val ic = ACTION_ICONS[text]; Row(verticalAlignment = Alignment.CenterVertically) { if (ic != null) { Glyph(ic, 18.dp, cs.onPrimary); Spacer(Modifier.width(8.dp)) }; Txt(text, T.body.copy(fontWeight = FontWeight.Medium), color = cs.onPrimary) } }
}

@Composable
fun TextAction(text: String, onClick: () -> Unit) {
    val ic = ACTION_ICONS[text]; val col = MaterialTheme.colorScheme.primary
    if (ic != null && text in ICON_ONLY) {   // icon only: the words are for the screen reader
        Box(Modifier.size(48.dp).clip(RoundedCornerShape(50)).clickable(onClick = onClick, role = androidx.compose.ui.semantics.Role.Button, onClickLabel = text).semantics { contentDescription = text }, contentAlignment = LocalIconAlign.current) { Glyph(ic, 24.dp, col) }
        return
    }
    Row(Modifier.heightIn(min = 48.dp).clickable(onClick = onClick, role = androidx.compose.ui.semantics.Role.Button).padding(end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        if (ic != null) { Glyph(ic, 16.dp, col); Spacer(Modifier.width(5.dp)) }
        Text(text, style = T.small.copy(fontWeight = FontWeight.Medium), color = col, maxLines = 1, softWrap = false)
    }
}

/** "‹ Settings" style link back to where you came from: the arrow is a drawn icon, flush with the left edge. */
@Composable
fun BackLink(text: String, onClick: () -> Unit) {
    val col = MaterialTheme.colorScheme.primary
    Row(Modifier.heightIn(min = 48.dp).clickable(onClick = onClick, role = androidx.compose.ui.semantics.Role.Button).padding(end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Glyph("back", 20.dp, col); Spacer(Modifier.width(4.dp))
        Text(text, style = T.small.copy(fontWeight = FontWeight.Medium), color = col, maxLines = 1, softWrap = false)
    }
}

/** A friendly empty page: the flame, a title and one line. */
@Composable
fun EmptyState(title: String, sub: String? = null) {
    Column(Modifier.fillMaxWidth().padding(vertical = 36.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.size(96.dp).clip(RoundedCornerShape(30.dp)).background(softFill()), contentAlignment = Alignment.Center) { Spark(56.dp) }
        Spacer(Modifier.height(16.dp))
        Txt(title, T.h2, modifier = Modifier.padding(horizontal = 24.dp))
        if (sub != null) { Spacer(Modifier.height(4.dp)); Txt(sub, T.small, muted = true, modifier = Modifier.padding(horizontal = 32.dp)) }
    }
}

/** An icon that changes into another one: the old one shrinks away while the new one grows in. */
@Composable
fun MorphGlyph(name: String, size: Dp = 20.dp, color: Color = MaterialTheme.colorScheme.onSurfaceVariant) {
    androidx.compose.animation.AnimatedContent(name, label = "glyph", transitionSpec = {
        if (!Fx.motion) androidx.compose.animation.EnterTransition.None togetherWith androidx.compose.animation.ExitTransition.None
        else (androidx.compose.animation.fadeIn(androidx.compose.animation.core.tween(200)) + androidx.compose.animation.scaleIn(androidx.compose.animation.core.tween(220), initialScale = .5f)) togetherWith (androidx.compose.animation.fadeOut(androidx.compose.animation.core.tween(140)) + androidx.compose.animation.scaleOut(androidx.compose.animation.core.tween(160), targetScale = .5f))
    }) { Glyph(it, size, color) }
}

/** A grey shape that breathes while something loads, so the page has its final shape from the start. */
@Composable
fun Skeleton(height: Dp, modifier: Modifier = Modifier, radius: Dp = 20.dp) {
    val tr = androidx.compose.animation.core.rememberInfiniteTransition(label = "skeleton")
    val a by tr.animateFloat(.35f, .75f, androidx.compose.animation.core.infiniteRepeatable(androidx.compose.animation.core.tween(900, easing = androidx.compose.animation.core.FastOutSlowInEasing), androidx.compose.animation.core.RepeatMode.Reverse), label = "a")
    Box(modifier.fillMaxWidth().height(height).clip(RoundedCornerShape(radius)).background(MaterialTheme.colorScheme.outline.copy(alpha = if (Fx.motion) a else .5f)))
}

/** When an item in a column moves to another place (the list was re-ordered), it glides there instead of jumping. */
fun Modifier.glide(): Modifier = composed {
    val scope = rememberCoroutineScope()
    var target by remember { mutableStateOf(androidx.compose.ui.unit.IntOffset.Zero) }
    var anim by remember { mutableStateOf<Animatable<androidx.compose.ui.unit.IntOffset, androidx.compose.animation.core.AnimationVector2D>?>(null) }
    this.onGloballyPositioned { c -> val p = c.positionInParent(); target = androidx.compose.ui.unit.IntOffset(p.x.roundToInt(), p.y.roundToInt()) }
        .offset {
            val a = anim ?: Animatable(target, androidx.compose.ui.unit.IntOffset.VectorConverter).also { anim = it }
            if (Fx.motion && a.targetValue != target) scope.launch { a.animateTo(target, spring(dampingRatio = 0.85f, stiffness = 300f)) } else if (!Fx.motion && a.targetValue != target) scope.launch { a.snapTo(target) }
            a.value - target
        }
}

/** One setting on one line: its name on the left, the chosen value on the right; a tap opens the choices as a small menu. */
@Composable
fun Pick(label: String, options: List<Pair<String, String>>, value: String, onPick: (String) -> Unit) {
    var open by remember { mutableStateOf(false) }
    val cs = MaterialTheme.colorScheme
    Box {
        Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).clip(RoundedCornerShape(12.dp)).clickable(role = androidx.compose.ui.semantics.Role.Button, onClickLabel = "Change $label") { open = true }.padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically) {
            Txt(label, T.body, modifier = Modifier.weight(1f))
            Txt(options.firstOrNull { it.second == value }?.first ?: options.firstOrNull()?.first.orEmpty(), T.body.copy(fontWeight = FontWeight.Medium), color = cs.primary)
            Spacer(Modifier.width(4.dp)); Glyph("chevd", 16.dp, cs.primary)
        }
        androidx.compose.material3.DropdownMenu(open, { open = false }, modifier = Modifier.background(cs.surfaceVariant), offset = androidx.compose.ui.unit.DpOffset(1000.dp, 0.dp)) {
            options.forEach { (l, v) ->
                androidx.compose.material3.DropdownMenuItem(text = { Txt(l, T.body.copy(fontWeight = if (v == value) FontWeight.SemiBold else FontWeight.Normal)) },
                    trailingIcon = { if (v == value) Glyph("check", 18.dp, cs.primary) }, onClick = { open = false; onPick(v) })
            }
        }
    }
}
