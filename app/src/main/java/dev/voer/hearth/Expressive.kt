package dev.voer.hearth

import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.AnimationSpec
import androidx.compose.animation.core.FastOutSlowInEasing
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.snap
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.composed
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.graphics.layer.drawLayer
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInWindow
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.Outline
import androidx.compose.ui.graphics.Shape
import androidx.compose.ui.graphics.TransformOrigin
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.abs
import kotlin.math.roundToInt

/* Material 3 Expressive motion for the "Material 3" style: everything that moves, moves on springs.
   Movement may overshoot (it is physical); colour and opacity never do. With "Motion" off every spec snaps. */
object Spr {
    /** Small, quick movements: a press, an icon, a chip. Bouncy. */
    fun <T> fast(): androidx.compose.animation.core.FiniteAnimationSpec<T> = if (Fx.motion) spring(dampingRatio = 0.6f, stiffness = 800f) else snap()
    /** The default for anything that travels: an indicator, a card arriving. */
    fun <T> move(): androidx.compose.animation.core.FiniteAnimationSpec<T> = if (Fx.motion) spring(dampingRatio = 0.8f, stiffness = 380f) else snap()
    /** Big things: a page. */
    fun <T> slow(): androidx.compose.animation.core.FiniteAnimationSpec<T> = if (Fx.motion) spring(dampingRatio = 0.8f, stiffness = 200f) else snap()
    /** Opacity and colour: critically damped, never overshoots. */
    fun <T> fade(): androidx.compose.animation.core.FiniteAnimationSpec<T> = if (Fx.motion) spring(dampingRatio = 1f, stiffness = 1600f) else snap()
}

/** Things that appear together arrive one after another: each one a little later than the one before. */
private object Stagger { var at = 0L; var n = 0
    fun next(): Long { val now = System.currentTimeMillis(); if (now - at > 140) n = 0; at = now; return (n++ * 38L).coerceAtMost(300L) } }

/** The expressive entrance: rises a little, grows into place and settles with a soft overshoot. */
fun Modifier.springIn(): Modifier = composed {
    if (LocalCalm.current || !Fx.motion) return@composed this
    val a = remember { Animatable(0f) }
    val o = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        delay(Stagger.next())
        launch { o.animateTo(1f, spring(dampingRatio = 1f, stiffness = 600f)) }
        a.animateTo(1f, spring(dampingRatio = 0.72f, stiffness = 320f))
    }
    graphicsLayer {
        alpha = o.value
        translationY = (1f - a.value) * 34.dp.toPx()
        val s = 0.94f + 0.06f * a.value
        scaleX = s; scaleY = s
        transformOrigin = TransformOrigin(0.5f, 0f)
    }
}

/**
 * The expressive press: the surface leans toward your finger and stretches a little that way, a soft light follows the
 * finger and a ring spreads from where you touched; on release it springs back past its rest and settles.
 * A scroll that takes over the touch lets it go at once, so lists never wobble while you scroll.
 */
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class)
fun Modifier.elastic(
    shape: Shape = RoundedCornerShape(16.dp),
    enabled: Boolean = true,
    stretch: Float = 0.06f,
    travel: Dp = 2.dp,
    role: Role? = Role.Button,
    label: String? = null,
    haptic: Boolean = true,
    onLong: (() -> Unit)? = null,
    onClick: () -> Unit,
): Modifier = composed {
    val hap = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val motion = Fx.motion
    var touch by remember { mutableStateOf<Offset?>(null) }
    var last by remember { mutableStateOf<Offset?>(null) }
    var origin by remember { mutableStateOf(Offset.Zero) }
    val ring = remember { Animatable(1f) }
    var tx by remember { mutableFloatStateOf(0f) }
    var ty by remember { mutableFloatStateOf(0f) }
    var sizeMax by remember { mutableFloatStateOf(1f) }
    val pressed = touch != null && motion
    val follow = spring<Float>(dampingRatio = 0.82f, stiffness = 550f)
    val release = spring<Float>(dampingRatio = 0.52f, stiffness = 350f)
    val dx by animateFloatAsState(if (motion) tx else 0f, if (pressed) follow else release, label = "edx")
    val dy by animateFloatAsState(if (motion) ty else 0f, if (pressed) follow else release, label = "edy")
    // a big card gives a little, a small button gives more: the same few pixels of travel either way
    val squeeze = with(LocalDensity.current) { (1f - 10.dp.toPx() / sizeMax).coerceIn(0.94f, 0.985f) }
    val scale by animateFloatAsState(if (pressed) squeeze else 1f, if (pressed) tween(90, easing = FastOutSlowInEasing) else spring(dampingRatio = 0.55f, stiffness = 420f), label = "esc")
    val glow by animateFloatAsState(if (pressed) 1f else 0f, tween(if (pressed) 120 else 280, easing = FastOutSlowInEasing), label = "eglow")
    val cs = MaterialTheme.colorScheme
    val dark = cs.surface.luminance() < .5f
    val light = if (dark) Color.White else cs.primary
    val src = remember { MutableInteractionSource() }
    val tap = { if (haptic) hap.performHapticFeedback(HapticFeedbackType.TextHandleMove); onClick() }

    this
        .pointerInput(enabled, motion) {
            if (!enabled || !motion) return@pointerInput
            awaitPointerEventScope {
                while (true) {
                    val e = awaitPointerEvent(PointerEventPass.Final)
                    val ch = e.changes.firstOrNull() ?: continue
                    val cx = size.width / 2f; val cy = size.height / 2f
                    sizeMax = maxOf(size.width, size.height).toFloat().coerceAtLeast(1f)
                    when {
                        ch.pressed && !ch.previousPressed -> {   // a new touch: start the ring where the finger is
                            origin = ch.position
                            scope.launch { ring.snapTo(0f); ring.animateTo(1f, tween(460, easing = FastOutSlowInEasing)) }
                            touch = ch.position; last = ch.position
                            if (cx > 0f && cy > 0f) { tx = ((ch.position.x - cx) / cx).coerceIn(-1f, 1f); ty = ((ch.position.y - cy) / cy).coerceIn(-1f, 1f) }
                        }
                        ch.pressed && ch.isConsumed && touch != null && (ch.position - (touch ?: ch.position)).getDistance() > viewConfiguration.touchSlop -> { touch = null; tx = 0f; ty = 0f }   // a scroll took it
                        ch.pressed && touch != null -> {
                            touch = ch.position; last = ch.position
                            if (cx > 0f && cy > 0f) { tx = ((ch.position.x - cx) / cx).coerceIn(-1f, 1f); ty = ((ch.position.y - cy) / cy).coerceIn(-1f, 1f) }
                        }
                        !ch.pressed -> { touch = null; tx = 0f; ty = 0f }
                    }
                }
            }
        }
        .graphicsLayer {
            val t = travel.toPx()
            val sx = 1f + stretch * abs(dx) - 0.03f * abs(dy)
            val sy = 1f + stretch * 1.06f * abs(dy) - 0.03f * abs(dx)
            transformOrigin = TransformOrigin((0.5f - 0.3f * dx).coerceIn(0.15f, 0.85f), (0.5f - 0.3f * dy).coerceIn(0.15f, 0.85f))
            scaleX = scale * sx; scaleY = scale * sy
            translationX = dx * t; translationY = dy * t
            this.shape = shape; clip = true
            compositingStrategy = CompositingStrategy.Offscreen
        }
        .drawWithContent {
            drawContent()
            if (glow < 0.01f && ring.value >= 1f) return@drawWithContent
            val outline = shape.createOutline(size, layoutDirection, this)
            fun fill(b: Brush) = when (outline) {
                is Outline.Rounded -> drawRoundRect(b, cornerRadius = outline.roundRect.topLeftCornerRadius)
                is Outline.Generic -> drawPath(outline.path, b)
                is Outline.Rectangle -> drawRect(b)
            }
            if (glow > 0.01f) {   // the light under your finger
                val c = touch ?: last ?: Offset(size.width / 2f, size.height / 2f)
                val r = if (dark) size.maxDimension * 0.75f else size.minDimension * 1.15f + size.maxDimension * 0.22f
                fill(Brush.radialGradient(
                    colorStops = arrayOf(0f to light.copy(alpha = (if (dark) .20f else .16f) * glow), 0.45f to light.copy(alpha = (if (dark) .08f else .07f) * glow), 1f to Color.Transparent),
                    center = c, radius = r.coerceAtLeast(1f)))
            }
            if (ring.value < 1f) {   // the ring that spreads from the touch
                val p = ring.value
                val r = size.maxDimension * 1.3f * p
                fill(Brush.radialGradient(
                    colorStops = arrayOf(0f to Color.Transparent, 0.55f to light.copy(alpha = .10f * (1f - p)), 0.85f to light.copy(alpha = .16f * (1f - p)), 1f to Color.Transparent),
                    center = origin, radius = r.coerceAtLeast(1f)))
            }
        }
        .semantics { if (label != null) contentDescription = label; if (role != null) this.role = role }
        .let { m ->
            if (onLong == null) m.clickable(interactionSource = src, indication = null, enabled = enabled, onClick = tap)
            else m.combinedClickable(interactionSource = src, indication = null, enabled = enabled, onClick = tap,
                onLongClick = { if (haptic) hap.performHapticFeedback(HapticFeedbackType.LongPress); onLong() })
        }
}

/**
 * Swipe a row sideways: the colour and icon of what will happen come out from under it, the icon grows when you have
 * pulled far enough (with a tick), and letting go does it and lets the row spring back. Pulling past the end
 * gets heavier, like a rubber band. [start] is the swipe to the right, [end] the swipe to the left.
 */
class SwipeAction(val icon: String, val label: String, val container: @Composable () -> Color, val ink: @Composable () -> Color, val run: () -> Unit)

@Composable
fun SwipeRow(start: SwipeAction?, end: SwipeAction?, shape: Shape, modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    if (!Fx.m3 || (start == null && end == null)) { Box(modifier) { content() }; return }
    val hap = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val x = remember { Animatable(0f) }
    var width by remember { mutableFloatStateOf(1f) }
    var armed by remember { mutableStateOf(false) }
    val threshold = with(LocalDensity.current) { 96.dp.toPx() }
    val side = if (x.value > 0f) start else if (x.value < 0f) end else null
    val pulled = (abs(x.value) / threshold).coerceIn(0f, 1.4f)
    val iconScale by animateFloatAsState(if (armed) 1.18f else 0.6f + 0.4f * pulled.coerceAtMost(1f), Spr.fast(), label = "swipeIcon")
    Box(modifier
        .pointerInput(start != null, end != null) {
            width = size.width.toFloat()
            detectHorizontalDragGestures(
                onDragEnd = {
                    val act = if (armed) (if (x.value > 0f) start else end) else null
                    armed = false
                    act?.run?.invoke()
                    scope.launch { x.animateTo(0f, spring(dampingRatio = 0.55f, stiffness = 380f)) }
                },
                onDragCancel = { armed = false; scope.launch { x.animateTo(0f, spring(dampingRatio = 0.55f, stiffness = 380f)) } },
            ) { ch, d ->
                ch.consume()
                val raw = x.value + d
                val allowed = when { raw > 0f && start == null -> 0f; raw < 0f && end == null -> 0f; else -> raw }
                // past the threshold every pixel of finger moves the row a little less
                val over = abs(allowed) - threshold
                val next = if (over > 0f && abs(d) > 0f && abs(raw) > abs(x.value)) x.value + d * (1f / (1f + over / (threshold * 0.6f))) else allowed
                scope.launch { x.snapTo(next.coerceIn(-width * 0.8f, width * 0.8f)) }
                val nowArmed = abs(next) >= threshold
                if (nowArmed != armed) { armed = nowArmed; if (nowArmed) hap.performHapticFeedback(HapticFeedbackType.LongPress) else hap.performHapticFeedback(HapticFeedbackType.TextHandleMove) }
            }
        }) {
        if (side != null) {
            val bg = side.container(); val ink = side.ink()
            Box(Modifier.matchParentSize().clip(shape).background(bg.copy(alpha = (0.35f + 0.65f * pulled.coerceAtMost(1f)))).padding(horizontal = 28.dp),
                contentAlignment = if (x.value > 0f) Alignment.CenterStart else Alignment.CenterEnd) {
                Box(Modifier.graphicsLayer { scaleX = iconScale; scaleY = iconScale; rotationZ = if (armed) (if (x.value > 0f) -8f else 8f) else 0f }) { Glyph(side.icon, 24.dp, ink) }
            }
        }
        Box(Modifier.offset { IntOffset(x.value.roundToInt(), 0) }.graphicsLayer {
            // the row tips a hair toward where it goes, as if it had weight
            rotationZ = (x.value / width.coerceAtLeast(1f)) * 2.2f
        }) { content() }
    }
}

/**
 * Frosted glass for the dock, done the way Android's own blur works: the page draws itself into a layer once per frame
 * ([backdropSource]), and the dock draws that same layer again behind itself, blurred by the GPU ([frostedGlass]).
 * Android 12 and newer; older phones get a slightly more opaque tint instead.
 */
class Backdrop { var layer: androidx.compose.ui.graphics.layer.GraphicsLayer? = null; var origin by mutableStateOf(Offset.Zero) }
val LocalBackdrop = androidx.compose.runtime.staticCompositionLocalOf<Backdrop?> { null }

fun Modifier.backdropSource(b: Backdrop): Modifier = composed {
    val layer = androidx.compose.ui.graphics.rememberGraphicsLayer()
    b.layer = layer
    this.onGloballyPositioned { b.origin = it.positionInWindow() }
        .drawWithContent { layer.record { this@drawWithContent.drawContent() }; drawLayer(layer) }
}

/** The glass itself: the page behind, blurred and brightened, a tint of the surface, a hairline edge and light catching its top. */
@Composable
fun GlassBack(shape: Shape, tint: Color, modifier: Modifier = Modifier) {
    val b = LocalBackdrop.current
    val cs = MaterialTheme.colorScheme
    val dark = cs.surface.luminance() < .5f
    var at by remember { mutableStateOf(Offset.Zero) }
    val blurPx = with(LocalDensity.current) { 30.dp.toPx() }
    val canBlur = android.os.Build.VERSION.SDK_INT >= 31 && b != null
    Box(modifier.onGloballyPositioned { at = it.positionInWindow() }.clip(shape)) {
        if (canBlur) Box(Modifier.matchParentSize()
            .graphicsLayer {
                renderEffect = androidx.compose.ui.graphics.BlurEffect(blurPx, blurPx, androidx.compose.ui.graphics.TileMode.Clamp)
                clip = true; this.shape = shape
            }
            .drawBehind {
                val l = b?.layer ?: return@drawBehind
                val o = b.origin - at
                drawContext.canvas.save(); drawContext.canvas.translate(o.x, o.y)
                drawLayer(l)
                drawContext.canvas.restore()
            })
        // the tint: thin where there is a blur to show, fuller where there is not
        Box(Modifier.matchParentSize().background(Brush.verticalGradient(listOf(tint.copy(alpha = if (canBlur) .55f else .9f), tint.copy(alpha = if (canBlur) .38f else .82f)))))
        // a hairline edge and the light on its top edge
        Box(Modifier.matchParentSize().drawWithContent {
            val outline = shape.createOutline(size, layoutDirection, this)
            val stroke = androidx.compose.ui.graphics.drawscope.Stroke(1.dp.toPx())
            val edge = Brush.verticalGradient(listOf(Color.White.copy(alpha = if (dark) .22f else .7f), Color.White.copy(alpha = if (dark) .04f else .2f), Color.Black.copy(alpha = if (dark) .12f else .04f)))
            when (outline) {
                is Outline.Rounded -> drawRoundRect(edge, cornerRadius = outline.roundRect.topLeftCornerRadius, style = stroke)
                is Outline.Generic -> drawPath(outline.path, edge, style = stroke)
                is Outline.Rectangle -> drawRect(edge, style = stroke)
            }
        })
    }
}

/** What an icon does when you tap its place in the dock. */
enum class Tap { Wiggle, Bounce, Rewind, Turn }

/**
 * The floating dock of Material 3: a pill that floats over the page, a round marker that glides to the chosen place and
 * can be dragged along it, every icon with its own little move when tapped, and next to it the add button, which only
 * the Chats place has: it grows out of the dock and the dock slides aside to make room.
 */
@Composable
fun FloatingDock(
    items: List<Pair<String, String>>,   // label to glyph
    taps: List<Tap>,
    selected: Int,
    onSelect: (Int) -> Unit,
    showAdd: Boolean,
    addLabel: String,
    onAdd: () -> Unit,
    modifier: Modifier = Modifier,
    addModifier: Modifier = Modifier,
    glass: Boolean = false,
) {
    val cs = MaterialTheme.colorScheme
    val dark = cs.surface.luminance() < .5f
    val hap = LocalHapticFeedback.current
    val itemW = 64.dp
    val dockW = itemW * items.size + 8.dp
    val dockH = 68.dp
    val gap = 10.dp
    val shift by animateDpAsState(if (showAdd) 0.dp else (dockH + gap) / 2, Spr.move(), label = "dockShift")
    val addAlpha by animateFloatAsState(if (showAdd) 1f else 0f, Spr.fade(), label = "addAlpha")
    val addScale by animateFloatAsState(if (showAdd) 1f else .5f, Spr.fast(), label = "addScale")
    val addTurn by animateFloatAsState(if (showAdd) 0f else -90f, Spr.move(), label = "addTurn")
    var drag by remember { mutableStateOf<Float?>(null) }   // where the finger holds the marker, in px from the dock's left
    val density = LocalDensity.current
    Box(modifier, contentAlignment = Alignment.Center) {
        Row(Modifier.width(dockW + gap + dockH), horizontalArrangement = Arrangement.spacedBy(gap), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier
                .graphicsLayer { translationX = shift.toPx() }
                .width(dockW).height(dockH)
                .shadow(if (dark) 10.dp else 14.dp, RoundedCornerShape(50), ambientColor = if (dark) Color.Black else cs.primary.copy(alpha = .3f), spotColor = if (dark) Color.Black else cs.primary.copy(alpha = .35f))
                .clip(RoundedCornerShape(50))
                .let { if (glass) it else it.background(if (dark) cs.surfaceContainerHigh else cs.surfaceContainerLow) }
                .pointerInput(items.size) {
                    val w = with(density) { itemW.toPx() }; val pad = with(density) { 4.dp.toPx() }
                    detectHorizontalDragGestures(
                        onDragStart = { o -> drag = o.x; hap.performHapticFeedback(HapticFeedbackType.TextHandleMove) },
                        onDragEnd = { drag?.let { d -> onSelect(((d - pad) / w).toInt().coerceIn(0, items.size - 1)) }; drag = null },
                        onDragCancel = { drag = null },
                    ) { ch, d -> ch.consume()
                        val before = drag ?: return@detectHorizontalDragGestures
                        val nx = (before + d).coerceIn(pad, pad + w * items.size)
                        if (((before - pad) / w).toInt() != ((nx - pad) / w).toInt()) hap.performHapticFeedback(HapticFeedbackType.TextHandleMove)
                        drag = nx
                    }
                }) {
                if (glass) GlassBack(RoundedCornerShape(50), if (dark) cs.surfaceContainerHighest else cs.surfaceContainerLow, Modifier.matchParentSize())
                BoxWithConstraints(Modifier.fillMaxSize().padding(horizontal = 4.dp)) {
                    val marker = 52.dp
                    val held = drag?.let { with(density) { (it.toDp() - 4.dp - marker / 2).coerceIn(0.dp, itemW * (items.size - 1) + (itemW - marker)) } }
                    val target = held ?: (itemW * selected + (itemW - marker) / 2)
                    val x by animateDpAsState(target, if (held != null) spring(dampingRatio = 1f, stiffness = 1400f) else Spr.move(), label = "dockMarker")
                    // the marker stretches with its speed, like a drop of something thick
                    val stretchX by animateFloatAsState(if (held != null) 1.18f else 1f, Spr.fast(), label = "dockStretch")
                    Box(Modifier.offset(x = x, y = (dockH - marker) / 2).size(marker).graphicsLayer { scaleX = stretchX; scaleY = 2f - stretchX }
                        .clip(CircleShape).background(cs.secondaryContainer))
                    Row(Modifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically) {
                        items.forEachIndexed { i, (label, glyph) ->
                            DockIcon(label, glyph, taps.getOrElse(i) { Tap.Bounce }, selected == i, itemW) { if (selected != i) hap.performHapticFeedback(HapticFeedbackType.TextHandleMove); onSelect(i) }
                        }
                    }
                }
            }
            Box(addModifier
                .size(dockH)
                .graphicsLayer { alpha = addAlpha; scaleX = addScale; scaleY = addScale; rotationZ = addTurn }
                .shadow(if (showAdd) 12.dp else 0.dp, RoundedCornerShape(22.dp), ambientColor = cs.primary.copy(alpha = .4f), spotColor = cs.primary.copy(alpha = .5f))
                .elastic(RoundedCornerShape(22.dp), enabled = showAdd, stretch = 0.08f, travel = 2.dp, label = addLabel, onClick = onAdd)
                .let { if (glass) it else it.background(cs.primaryContainer) },
                contentAlignment = Alignment.Center) {
                if (glass) GlassBack(RoundedCornerShape(22.dp), cs.primaryContainer, Modifier.matchParentSize())
                Glyph("plus", 28.dp, cs.onPrimaryContainer)
            }
        }
    }
}

@Composable
private fun RowScope.DockIcon(label: String, glyph: String, tap: Tap, on: Boolean, w: Dp, onClick: () -> Unit) {
    val cs = MaterialTheme.colorScheme
    var taps by remember { mutableIntStateOf(0) }
    val rot = remember { Animatable(0f) }
    val pop = remember { Animatable(1f) }
    val scale by animateFloatAsState(if (on) 1f else .92f, Spr.fast(), label = "dockScale")
    val ink by androidx.compose.animation.animateColorAsState(if (on) cs.onSecondaryContainer else cs.onSurfaceVariant, Spr.fade(), label = "dockInk")
    LaunchedEffect(taps) {
        if (taps == 0 || !Fx.motion) return@LaunchedEffect
        rot.snapTo(0f)
        when (tap) {
            Tap.Wiggle -> { rot.animateTo(-10f, tween(70)); rot.animateTo(9f, tween(90)); rot.animateTo(-5f, tween(80)); rot.animateTo(0f, spring(0.5f, 600f)) }
            Tap.Bounce -> { pop.animateTo(.78f, tween(90)); pop.animateTo(1f, spring(dampingRatio = 0.35f, stiffness = 700f)) }
            Tap.Rewind -> { rot.animateTo(-360f, spring(dampingRatio = 0.62f, stiffness = 140f)); rot.snapTo(0f) }
            Tap.Turn -> { rot.animateTo(120f, spring(dampingRatio = 0.5f, stiffness = 220f)); rot.snapTo(0f) }
        }
    }
    Box(Modifier.width(w).fillMaxSize()
        .semantics { contentDescription = label; this.selected = on; role = Role.Tab }
        .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { taps++; onClick() },
        contentAlignment = Alignment.Center) {
        Box(Modifier.graphicsLayer { val s = scale * pop.value; scaleX = s; scaleY = s; rotationZ = rot.value }) { Glyph(glyph, 26.dp, ink) }
    }
}

/** Room under the content of a page so the floating dock never covers its last line. */
val DOCK_SPACE = 104.dp

/* ── Expressive shapes: soft polar shapes (cookie, flower, burst, pill), drawn as one path; two of them blend into each other point by point ── */
private val SHAPES: List<(Double) -> Double> = listOf(
    { t -> 0.9 + 0.1 * kotlin.math.cos(9 * t) },                                   // cookie, nine soft lobes
    { t -> 0.82 + 0.18 * Math.pow(kotlin.math.abs(kotlin.math.cos(3 * t)), 0.8) },  // flower
    { t -> 0.86 + 0.14 * kotlin.math.cos(4 * t) },                                  // four-sided cookie
    { t -> 0.84 + 0.16 * kotlin.math.cos(12 * t) },                                 // burst
    { t -> 0.8 / Math.pow(Math.pow(kotlin.math.abs(kotlin.math.cos(t)), 4.0) + Math.pow(kotlin.math.abs(kotlin.math.sin(t)), 4.0), 0.25) },   // squircle
)

private fun shapePath(size: androidx.compose.ui.geometry.Size, a: Int, b: Int, mix: Float): androidx.compose.ui.graphics.Path {
    val p = androidx.compose.ui.graphics.Path()
    val r0 = minOf(size.width, size.height) / 2f; val cx = size.width / 2f; val cy = size.height / 2f
    val n = 96
    for (i in 0..n) {
        val t = 2 * Math.PI * i / n - Math.PI / 2
        val r = r0 * ((1 - mix) * SHAPES[a](t) + mix * SHAPES[b](t)).toFloat()
        val x = cx + r * kotlin.math.cos(t).toFloat(); val y = cy + r * kotlin.math.sin(t).toFloat()
        if (i == 0) p.moveTo(x, y) else p.lineTo(x, y)
    }
    p.close(); return p
}

/** The soft nine-sided "cookie" of Material 3 Expressive, as a quiet backdrop (behind the usage ring). */
@Composable
fun Cookie(modifier: Modifier, color: Color) = androidx.compose.foundation.Canvas(modifier) { drawPath(shapePath(size, 0, 0, 0f), color) }

/** The Material 3 Expressive loading indicator: one shape that turns and slowly changes into the next. Still when motion is off. */
@Composable
fun MorphLoader(size: Dp = 24.dp, color: Color = MaterialTheme.colorScheme.primary) {
    val tr = androidx.compose.animation.core.rememberInfiniteTransition(label = "morph")
    val phase by tr.animateFloat(0f, SHAPES.size.toFloat(), androidx.compose.animation.core.infiniteRepeatable(tween(SHAPES.size * 1100, easing = androidx.compose.animation.core.LinearEasing)), label = "phase")
    val turn by tr.animateFloat(0f, 360f, androidx.compose.animation.core.infiniteRepeatable(tween(3600, easing = androidx.compose.animation.core.LinearEasing)), label = "turn")
    androidx.compose.foundation.Canvas(Modifier.size(size).graphicsLayer { if (Fx.motion) rotationZ = turn }) {
        val ph = if (Fx.motion) phase else 0f
        val a = ph.toInt() % SHAPES.size; val f = ph - ph.toInt()
        val ease = (f * f * (3 - 2 * f)).let { e -> if (f < .55f) 0f else ((f - .55f) / .45f).let { it * it * (3 - 2 * it) } }   // holds each shape, then blends into the next
        drawPath(shapePath(this.size, a, (a + 1) % SHAPES.size, ease), color)
    }
}
