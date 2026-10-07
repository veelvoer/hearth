package dev.voer.hearth

import android.Manifest
import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.content.pm.PackageManager
import android.media.AudioManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat

private tailrec fun Context.activity(): Activity? = when (this) { is Activity -> this; is ContextWrapper -> baseContext.activity(); else -> null }

private fun mmss(ms: Long): String { val s = (ms / 1000).coerceAtLeast(0); return "%02d:%02d".format(s / 60, s % 60) }

private fun DrawScope.micIcon(c: Color, muted: Boolean) {
    val w = size.width; val sw = w * .09f
    drawRoundRect(c, Offset(w * .36f, w * .06f), Size(w * .28f, w * .5f), CornerRadius(w * .14f))
    drawArc(c, 0f, 180f, false, Offset(w * .22f, w * .26f), Size(w * .56f, w * .42f), style = Stroke(sw, cap = StrokeCap.Round))
    drawLine(c, Offset(w * .5f, w * .68f), Offset(w * .5f, w * .9f), sw, StrokeCap.Round)
    if (muted) drawLine(c, Offset(w * .12f, w * .92f), Offset(w * .88f, w * .08f), sw * 1.2f, StrokeCap.Round)
}

private fun DrawScope.speakerIcon(c: Color) {
    val w = size.width; val sw = w * .08f
    val p = Path().apply {
        moveTo(w * .08f, w * .38f); lineTo(w * .28f, w * .38f); lineTo(w * .5f, w * .18f); lineTo(w * .5f, w * .82f)
        lineTo(w * .28f, w * .62f); lineTo(w * .08f, w * .62f); close()
    }
    drawPath(p, c)
    drawArc(c, -50f, 100f, false, Offset(w * .36f, w * .3f), Size(w * .3f, w * .4f), style = Stroke(sw, cap = StrokeCap.Round))
    drawArc(c, -55f, 110f, false, Offset(w * .32f, w * .14f), Size(w * .56f, w * .72f), style = Stroke(sw, cap = StrokeCap.Round))
}

private fun DrawScope.hangUpIcon(c: Color) {
    val w = size.width
    drawArc(c, 205f, 130f, false, Offset(w * .04f, w * .34f), Size(w * .92f, w * .9f), style = Stroke(w * .2f, cap = StrokeCap.Round))
}

@Composable
private fun RoundButton(label: String, on: Boolean, big: Boolean = false, danger: Boolean = false, onClick: () -> Unit, icon: @Composable () -> Unit) {
    val cs = MaterialTheme.colorScheme
    val size = if (big) 72.dp else 64.dp
    Column(horizontalAlignment = Alignment.CenterHorizontally) {
        Box(Modifier.size(size).clip(CircleShape)
            .background(if (danger) Color(0xFFB3402A) else if (on) cs.primary else cs.surfaceVariant)
            .then(if (!on && !danger) Modifier.border(1.dp, cs.outline, CircleShape) else Modifier)
            .clickable(onClick = onClick).semantics { contentDescription = label }, contentAlignment = Alignment.Center) { icon() }
        Spacer(Modifier.height(8.dp))
        Txt(label, T.small, muted = true)
    }
}

@Composable
fun CallScreen(m: Machine, s: Sess, mode: String, onClose: () -> Unit) {
    val c = LocalContext.current
    val cs = MaterialTheme.colorScheme
    val perm = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        if (ok) CallSession.start(c, m, s, mode) else CallSession.error = "Microphone permission is needed."
    }
    LaunchedEffect(Unit) {
        if (!CallSession.active) {
            if (ContextCompat.checkSelfPermission(c, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) CallSession.start(c, m, s, mode)
            else perm.launch(Manifest.permission.RECORD_AUDIO)
        }
    }
    DisposableEffect(Unit) {
        val a = c.activity()
        a?.volumeControlStream = AudioManager.STREAM_VOICE_CALL
        onDispose { a?.volumeControlStream = AudioManager.USE_DEFAULT_STREAM_TYPE }
    }
    // the call ends -> leave the screen
    var wasActive by remember { mutableStateOf(false) }
    LaunchedEffect(CallSession.active) { if (CallSession.active) wasActive = true else if (wasActive) onClose() }
    val now by tick30()
    val phase = CallSession.phase
    val status = when {
        CallSession.muted -> "Muted"
        phase == Phase.Connecting -> "Connecting…"
        phase == Phase.Listening -> "Listening"
        phase == Phase.Thinking -> CallSession.status.ifEmpty { "Thinking…" }
        else -> "Speaking"
    }
    val ring = if (phase == Phase.Listening && !CallSession.muted) CallSession.level else if (phase == Phase.Speaking) .45f else 0f

    Column(Modifier.fillMaxSize().padding(horizontal = 24.dp, vertical = 12.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
            Label("Hearth", Modifier.weight(1f))
            Txt(mmss(now - CallSession.startedAt), T.small.copy(fontFamily = androidx.compose.ui.text.font.FontFamily.Monospace), muted = true)
        }
        Spacer(Modifier.weight(1f))
        Box(Modifier.size(240.dp), contentAlignment = Alignment.Center) {
            Canvas(Modifier.fillMaxSize()) {
                val base = size.minDimension * .3f
                drawCircle(cs.primary.copy(alpha = .10f + ring * .12f), base * (1.35f + ring * .45f))
                drawCircle(cs.primary.copy(alpha = .16f + ring * .16f), base * (1.1f + ring * .25f))
            }
            Box(Modifier.graphicsLayer { val sc = 1f + ring * .12f; scaleX = sc; scaleY = sc }) { Spark(110.dp) }
        }
        Spacer(Modifier.height(18.dp))
        Txt("Claude", T.title)
        Txt(s.title, T.small, muted = true, modifier = Modifier.padding(top = 2.dp))
        Spacer(Modifier.height(10.dp))
        Txt(status, T.body, muted = true)
        CallSession.error?.let { Spacer(Modifier.height(6.dp)); Txt(it, T.small, color = cs.primary, modifier = Modifier.padding(horizontal = 12.dp)) }
        if (CallSession.captions) {
            Spacer(Modifier.height(14.dp))
            Column(Modifier.fillMaxWidth().heightIn(max = 150.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                CallSession.lines.takeLast(3).forEach { (you, t) -> Txt(t, T.small, muted = you, modifier = Modifier.fillMaxWidth()) }
            }
        }
        Spacer(Modifier.weight(1f))
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceEvenly) {
            RoundButton("Mute", CallSession.muted, onClick = { CallSession.muted = !CallSession.muted }) {
                Canvas(Modifier.size(28.dp)) { micIcon(if (CallSession.muted) cs.onPrimary else cs.onSurface, CallSession.muted) }
            }
            RoundButton("Speaker", CallSession.speakerOn, onClick = { CallSession.toggleSpeaker() }) {
                Canvas(Modifier.size(28.dp)) { speakerIcon(if (CallSession.speakerOn) cs.onPrimary else cs.onSurface) }
            }
            RoundButton(if (CallSession.lang == "nl") "Nederlands" else "English", false, onClick = {
                CallSession.lang = if (CallSession.lang == "nl") "en" else "nl"; Store.setVoiceLang(c, CallSession.lang)
            }) { Txt(CallSession.lang.uppercase(), T.small.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold), color = cs.onSurface) }
            RoundButton("Captions", CallSession.captions, onClick = { CallSession.captions = !CallSession.captions }) {
                Txt("CC", T.small.copy(fontWeight = androidx.compose.ui.text.font.FontWeight.Bold), color = if (CallSession.captions) cs.onPrimary else cs.onSurface)
            }
        }
        Spacer(Modifier.height(26.dp))
        RoundButton("End", on = false, big = true, danger = true, onClick = { CallSession.end() }) {
            Canvas(Modifier.size(32.dp)) { hangUpIcon(Color.White) }
        }
        Spacer(Modifier.height(10.dp))
    }
}

@Composable
private fun tick30(): State<Long> = produceState(System.currentTimeMillis()) {
    while (true) { value = System.currentTimeMillis(); kotlinx.coroutines.delay(1000) }
}
