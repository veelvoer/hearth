package dev.voer.hearth

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
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
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Small line icons, drawn here so the app needs no icon library. */
@Composable
fun Glyph(name: String, size: Dp = 20.dp, color: Color = MaterialTheme.colorScheme.onSurfaceVariant) {
    Canvas(Modifier.size(size)) {
        val u = this.size.width / 24f
        val st = Stroke(width = 1.8f * u, cap = StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round)
        fun line(a: Float, b: Float, c: Float, d: Float) = drawLine(color, Offset(a * u, b * u), Offset(c * u, d * u), 1.8f * u, StrokeCap.Round)
        when (name) {
            "info" -> { drawCircle(color, 9f * u, Offset(12 * u, 12 * u), style = st); line(12f, 11f, 12f, 17f); line(12f, 7.6f, 12f, 7.7f) }
            "phone" -> { drawRoundRect(color, Offset(7 * u, 2 * u), Size(10 * u, 20 * u), CornerRadius(2.5f * u), style = st); line(11f, 18.5f, 13f, 18.5f) }
            "server" -> { drawRoundRect(color, Offset(3 * u, 4 * u), Size(18 * u, 6 * u), CornerRadius(1.5f * u), style = st); drawRoundRect(color, Offset(3 * u, 14 * u), Size(18 * u, 6 * u), CornerRadius(1.5f * u), style = st) }
            "bell" -> { val p = Path().apply { moveTo(6 * u, 16 * u); lineTo(6 * u, 11 * u); cubicTo(6 * u, 4 * u, 18 * u, 4 * u, 18 * u, 11 * u); lineTo(18 * u, 16 * u); lineTo(20 * u, 18 * u); lineTo(4 * u, 18 * u); close() }; drawPath(p, color, style = st); line(10f, 21f, 14f, 21f) }
            "mic" -> { drawRoundRect(color, Offset(9 * u, 3 * u), Size(6 * u, 11 * u), CornerRadius(3 * u), style = st); line(12f, 18f, 12f, 21f); val p = Path().apply { moveTo(5 * u, 11 * u); cubicTo(5 * u, 20 * u, 19 * u, 20 * u, 19 * u, 11 * u) }; drawPath(p, color, style = st) }
            "sync" -> { val p = Path().apply { moveTo(4 * u, 9 * u); cubicTo(7 * u, 1 * u, 17 * u, 2 * u, 20 * u, 8 * u) }; drawPath(p, color, style = st); val q = Path().apply { moveTo(20 * u, 15 * u); cubicTo(17 * u, 23 * u, 7 * u, 22 * u, 4 * u, 16 * u) }; drawPath(q, color, style = st); line(20f, 4f, 20f, 8f); line(20f, 8f, 16f, 8f); line(4f, 20f, 4f, 16f); line(4f, 16f, 8f, 16f) }
            "support" -> { drawCircle(color, 9f * u, Offset(12 * u, 12 * u), style = st); line(12f, 14f, 12f, 14.5f); val p = Path().apply { moveTo(9.5f * u, 9.5f * u); cubicTo(9.5f * u, 6.5f * u, 14.5f * u, 6.5f * u, 14.5f * u, 9.5f * u); cubicTo(14.5f * u, 11.5f * u, 12 * u, 11.5f * u, 12 * u, 13 * u) }; drawPath(p, color, style = st) }
            "paint" -> { drawCircle(color, 9f * u, Offset(12 * u, 12 * u), style = st); drawCircle(color, 1.2f * u, Offset(8 * u, 11 * u)); drawCircle(color, 1.2f * u, Offset(12 * u, 7.5f * u)); drawCircle(color, 1.2f * u, Offset(16 * u, 10 * u)) }
            "tour" -> { drawCircle(color, 9f * u, Offset(12 * u, 12 * u), style = st); val p = Path().apply { moveTo(15.5f * u, 8.5f * u); lineTo(13.5f * u, 13.5f * u); lineTo(8.5f * u, 15.5f * u); lineTo(10.5f * u, 10.5f * u); close() }; drawPath(p, color, style = st) }
            "voice" -> { line(4f, 10f, 4f, 14f); line(8f, 6f, 8f, 18f); line(12f, 3f, 12f, 21f); line(16f, 7f, 16f, 17f); line(20f, 10f, 20f, 14f) }
            "clock" -> { drawCircle(color, 9f * u, Offset(12 * u, 12 * u), style = st); line(12f, 7f, 12f, 12f); line(12f, 12f, 15.5f, 14f) }
            "widget" -> { drawRoundRect(color, Offset(3 * u, 3 * u), Size(7 * u, 7 * u), CornerRadius(1.5f * u), style = st); drawRoundRect(color, Offset(14 * u, 3 * u), Size(7 * u, 7 * u), CornerRadius(1.5f * u), style = st); drawRoundRect(color, Offset(3 * u, 14 * u), Size(7 * u, 7 * u), CornerRadius(1.5f * u), style = st); drawRoundRect(color, Offset(14 * u, 14 * u), Size(7 * u, 7 * u), CornerRadius(1.5f * u), style = st) }
            else -> drawCircle(color, 9f * u, Offset(12 * u, 12 * u), style = st)
        }
    }
}

/** The little (i): tap it for the explanation instead of reading a paragraph. */
@Composable
fun InfoDot(title: String, text: String) {
    var open by remember { mutableStateOf(false) }
    Box(Modifier.size(32.dp).clip(RoundedCornerShape(50)).clickable { open = true }, contentAlignment = Alignment.Center) { Glyph("info", 18.dp) }
    if (open) AlertDialog(onDismissRequest = { open = false }, confirmButton = { TextAction("Got it") { open = false } }, title = { Txt(title, T.h2) }, text = { Txt(text, T.body) },
        containerColor = MaterialTheme.colorScheme.surfaceVariant)
}

/** A card heading: icon, title and the (i). */
@Composable
fun Head(icon: String, title: String, tip: String? = null, modifier: Modifier = Modifier, trailing: @Composable RowScope.() -> Unit = {}) =
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Glyph(icon, 18.dp); Spacer(Modifier.width(8.dp)); Label(title, Modifier.weight(1f))
        if (tip != null) InfoDot(title, tip)
        trailing()
    }
