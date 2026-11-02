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
import androidx.compose.ui.graphics.drawscope.Fill
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.drawscope.withTransform
import androidx.compose.ui.graphics.vector.PathParser
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

/** Small line icons, drawn here so the app needs no icon library. */
@Composable
fun Glyph(name: String, size: Dp = 20.dp, color: Color = MaterialTheme.colorScheme.onSurfaceVariant) {
    Canvas(Modifier.size(size)) {
        val u = this.size.width / 24f
        val st = Stroke(width = 1.8f * u, cap = StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round)
        fun line(a: Float, b: Float, c: Float, d: Float) = drawLine(color, Offset(a * u, b * u), Offset(c * u, d * u), 1.8f * u, StrokeCap.Round)
        fun svg(d: String, fill: Boolean = false) = withTransform({ scale(u, u, Offset.Zero) }) {
            drawPath(PathParser().parsePathString(d).toPath(), color, style = if (fill) Fill else Stroke(1.8f, cap = StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
        }
        when (name) {
            "pin" -> svg("M12 17v5 M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z")
            "edit" -> svg("M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5z")
            "chevd" -> svg("M6 9l6 6 6-6")
            "chevu" -> svg("M6 15l6-6 6 6")
            "chevr" -> svg("M9 6l6 6-6 6")
            "key" -> svg("M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4")
            "shield" -> svg("M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z")
            "chip" -> svg("M6 4h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M9 9h6v6H9z M9 1v3 M15 1v3 M9 20v3 M15 20v3 M20 9h3 M20 14h3 M1 9h3 M1 14h3")
            "plug" -> svg("M12 22v-5 M9 8V2 M15 8V2 M18 8v5a4 4 0 0 1-4 4h-4a4 4 0 0 1-4-4V8z")
            "sparkle" -> svg("M12 2l2.4 7.6L22 12l-7.6 2.4L12 22l-2.4-7.6L2 12l7.6-2.4z")
            "gauge" -> svg("M12 14l4-4 M3.34 19a10 10 0 1 1 17.32 0")
            "contrast" -> { drawCircle(color, 9f * u, Offset(12 * u, 12 * u), style = st); svg("M12 3a9 9 0 0 1 0 18z", fill = true) }
            "clip" -> svg("M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48")
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
            "share" -> { drawCircle(color, 2.6f * u, Offset(18 * u, 5.5f * u), style = st); drawCircle(color, 2.6f * u, Offset(6 * u, 12 * u), style = st); drawCircle(color, 2.6f * u, Offset(18 * u, 18.5f * u), style = st); line(8.3f, 10.8f, 15.7f, 6.7f); line(8.3f, 13.2f, 15.7f, 17.3f) }
            "sliders" -> { line(4f, 7f, 20f, 7f); line(4f, 17f, 20f, 17f); drawCircle(color, 2.4f * u, Offset(9 * u, 7 * u), style = st); drawCircle(color, 2.4f * u, Offset(15 * u, 17 * u), style = st) }
            "trash" -> { line(4f, 7f, 20f, 7f); line(9f, 7f, 9f, 4f); line(9f, 4f, 15f, 4f); line(15f, 4f, 15f, 7f); line(6f, 7f, 7f, 20f); line(7f, 20f, 17f, 20f); line(17f, 20f, 18f, 7f) }
            "plus" -> { line(12f, 5f, 12f, 19f); line(5f, 12f, 19f, 12f) }
            "x" -> { line(6f, 6f, 18f, 18f); line(18f, 6f, 6f, 18f) }
            "check" -> { val p = Path().apply { moveTo(5 * u, 12.5f * u); lineTo(10 * u, 17.5f * u); lineTo(19 * u, 7 * u) }; drawPath(p, color, style = st) }
            "back" -> { val p = Path().apply { moveTo(15 * u, 5 * u); lineTo(8 * u, 12 * u); lineTo(15 * u, 19 * u) }; drawPath(p, color, style = st) }
            "mail" -> { drawRoundRect(color, Offset(3 * u, 5 * u), Size(18 * u, 14 * u), CornerRadius(2.5f * u), style = st); val p = Path().apply { moveTo(3.5f * u, 7.5f * u); lineTo(12 * u, 13.5f * u); lineTo(20.5f * u, 7.5f * u) }; drawPath(p, color, style = st) }
            "copy" -> { drawRoundRect(color, Offset(8.5f * u, 8.5f * u), Size(11.5f * u, 11.5f * u), CornerRadius(2f * u), style = st); val p = Path().apply { moveTo(4.5f * u, 15.5f * u); lineTo(4.5f * u, 5.5f * u); cubicTo(4.5f * u, 4.5f * u, 5 * u, 4 * u, 6 * u, 4 * u); lineTo(15.5f * u, 4 * u) }; drawPath(p, color, style = st) }
            "people" -> { drawCircle(color, 3.2f * u, Offset(9 * u, 8 * u), style = st); val p = Path().apply { moveTo(3 * u, 19 * u); cubicTo(3 * u, 13 * u, 15 * u, 13 * u, 15 * u, 19 * u) }; drawPath(p, color, style = st); drawCircle(color, 2.6f * u, Offset(17 * u, 8.5f * u), style = st); val q = Path().apply { moveTo(17 * u, 13.5f * u); cubicTo(20 * u, 13.5f * u, 21 * u, 16 * u, 21 * u, 18.5f * u) }; drawPath(q, color, style = st) }
            "chat" -> { val p = Path().apply { moveTo(5 * u, 5 * u); lineTo(19 * u, 5 * u); cubicTo(20.5f * u, 5 * u, 21 * u, 5.5f * u, 21 * u, 7 * u); lineTo(21 * u, 14 * u); cubicTo(21 * u, 15.5f * u, 20.5f * u, 16 * u, 19 * u, 16 * u); lineTo(11 * u, 16 * u); lineTo(7 * u, 20 * u); lineTo(7 * u, 16 * u); lineTo(5 * u, 16 * u); cubicTo(3.5f * u, 16 * u, 3 * u, 15.5f * u, 3 * u, 14 * u); lineTo(3 * u, 7 * u); cubicTo(3 * u, 5.5f * u, 3.5f * u, 5 * u, 5 * u, 5 * u); close() }; drawPath(p, color, style = st) }
            "home" -> { val p = Path().apply { moveTo(3.5f * u, 11 * u); lineTo(12 * u, 4 * u); lineTo(20.5f * u, 11 * u); moveTo(6 * u, 9.5f * u); lineTo(6 * u, 20 * u); lineTo(18 * u, 20 * u); lineTo(18 * u, 9.5f * u); moveTo(10 * u, 20 * u); lineTo(10 * u, 14 * u); lineTo(14 * u, 14 * u); lineTo(14 * u, 20 * u) }; drawPath(p, color, style = st) }
            "history" -> svg("M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8 M3 3v5h5 M12 7v5l4 2")
            "gear" -> { svg("M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"); drawCircle(color, 3f * u, Offset(12 * u, 12 * u), style = st) }
            "link" -> { val p = Path().apply { moveTo(10 * u, 14 * u); cubicTo(8 * u, 12 * u, 8 * u, 9.5f * u, 10 * u, 7.5f * u); lineTo(13 * u, 4.5f * u); cubicTo(15 * u, 2.5f * u, 18 * u, 3.5f * u, 19 * u, 5.5f * u); moveTo(14 * u, 10 * u); cubicTo(16 * u, 12 * u, 16 * u, 14.5f * u, 14 * u, 16.5f * u); lineTo(11 * u, 19.5f * u); cubicTo(9 * u, 21.5f * u, 6 * u, 20.5f * u, 5 * u, 18.5f * u) }; drawPath(p, color, style = st) }
            "play" -> { val p = Path().apply { moveTo(8 * u, 5 * u); lineTo(19 * u, 12 * u); lineTo(8 * u, 19 * u); close() }; drawPath(p, color, style = st) }
            "download" -> { line(12f, 4f, 12f, 15f); val p = Path().apply { moveTo(7 * u, 11 * u); lineTo(12 * u, 16 * u); lineTo(17 * u, 11 * u) }; drawPath(p, color, style = st); line(5f, 20f, 19f, 20f) }
            "lock" -> { drawRoundRect(color, Offset(5 * u, 11 * u), Size(14 * u, 9 * u), CornerRadius(2f * u), style = st); val p = Path().apply { moveTo(8 * u, 11 * u); lineTo(8 * u, 8 * u); cubicTo(8 * u, 3 * u, 16 * u, 3 * u, 16 * u, 8 * u); lineTo(16 * u, 11 * u) }; drawPath(p, color, style = st) }
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

/** A button that is only an icon (header actions). Tap target is 40dp. */
@Composable
fun IconAction(glyph: String, desc: String, onClick: () -> Unit) =
    Box(Modifier.size(48.dp).clip(RoundedCornerShape(50)).clickable(onClick = onClick, role = androidx.compose.ui.semantics.Role.Button, onClickLabel = desc).semantics { contentDescription = desc }, contentAlignment = Alignment.Center) { Glyph(glyph, 24.dp, MaterialTheme.colorScheme.primary) }

/** The familiar icon that goes with a one-word button. */
/** These buttons show only their icon (the words stay as the screen reader label); every other button keeps its words. */
val ICON_ONLY = setOf("Cancel", "Close", "Not now", "Got it", "Done", "Save", "Back", "Copy", "Share", "Attach", "Tour", "Setup", "What changed", "Look again", "Refresh", "Check for updates", "Check again", "Remove", "Add", "Write an email")
val ACTION_ICONS = mapOf("Share" to "share", "Talk" to "mic", "Options" to "sliders", "Done" to "check", "Remove" to "trash", "Add" to "plus", "Look again" to "sync", "Refresh" to "sync",
    "Check for updates" to "sync", "Check again" to "sync", "Test call" to "phone", "Cancel" to "x", "Close" to "x", "Not now" to "x", "Got it" to "check", "Back" to "back", "Tour" to "tour",
    "Write an email" to "mail", "Contact" to "mail", "Attach" to "clip", "Copy" to "copy", "Share" to "share", "Pin to top" to "pin", "Unpin" to "pin", "Rename" to "edit", "Pin" to "lock", "Allow" to "check", "Deny" to "x", "Connect" to "plus", "Update" to "download",
    "Make invite link" to "share", "Share link" to "share", "Stop sharing" to "trash", "Join" to "people", "Enter the address myself" to "link", "What changed" to "history", "Restart" to "sync", "Delete" to "trash", "Delete this chat…" to "trash", "Revoke" to "x", "Look again" to "sync", "Show all" to "plus", "Remove step" to "trash", "Open folder" to "folder", "Set up a workflow" to "sliders", "Add to tasks" to "check", "Fix with Claude" to "tour", "Save" to "check", "Install" to "download", "Continue" to "play", "Setup" to "play", "Background access" to "clock", "Full-screen calls" to "phone")
