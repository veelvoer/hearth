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

private val GLYPH_PATHS = HashMap<String, Path>()   // parsed once, drawn many times

/** Line icons from Lucide (lucide.dev, ISC license), drawn from their path data so the app needs no icon library. */
@Composable
fun Glyph(name: String, size: Dp = 20.dp, color: Color = MaterialTheme.colorScheme.onSurfaceVariant) {
    Canvas(Modifier.size(size)) {
        val u = this.size.width / 24f
        fun svg(d: String) = withTransform({ scale(u, u, Offset.Zero) }) {
            drawPath(GLYPH_PATHS.getOrPut(d) { PathParser().parsePathString(d).toPath() }, color, style = Stroke(2f, cap = StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
        }
        if (name == "stop") { drawRoundRect(color, Offset(7 * u, 7 * u), Size(10 * u, 10 * u), CornerRadius(2f * u)); return@Canvas }   // the stop button is a solid square
        svg(LUCIDE[name] ?: LUCIDE.getValue("circle"))
    }
}

/** Where the picture sits inside a 48dp button: a button at the right edge of a row hugs that edge, one at the left edge hugs the left, so every picture lines up with the text above and below it. */
val LocalIconAlign = androidx.compose.runtime.compositionLocalOf<Alignment> { Alignment.Center }

/** The little (i): tap it for the explanation instead of reading a paragraph. */
@Composable
fun InfoDot(title: String, text: String) {
    var open by remember { mutableStateOf(false) }
    Box(Modifier.size(48.dp).clip(RoundedCornerShape(50)).clickable { open = true }, contentAlignment = LocalIconAlign.current) { Glyph("info", 20.dp) }
    if (open) AlertDialog(onDismissRequest = { open = false }, confirmButton = { TextAction("Got it") { open = false } }, title = { Txt(title, T.h2) }, text = { Txt(text, T.body) },
        containerColor = MaterialTheme.colorScheme.surfaceVariant)
}

/** A card heading: icon, title and the (i). */
@Composable
fun Head(icon: String, title: String, tip: String? = null, modifier: Modifier = Modifier, trailing: @Composable RowScope.() -> Unit = {}) =
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Glyph(icon, 18.dp); Spacer(Modifier.width(8.dp)); Label(title, Modifier.weight(1f))
        androidx.compose.runtime.CompositionLocalProvider(LocalIconAlign provides Alignment.CenterEnd) {   // everything at the right end hugs the right edge
            if (tip != null) InfoDot(title, tip)
            trailing()
        }
    }

/** A button that is only an icon (header actions). Tap target is 40dp. */
@Composable
fun IconAction(glyph: String, desc: String, align: Alignment = LocalIconAlign.current, size: Dp = 48.dp, onClick: () -> Unit) =
    Box(Modifier.size(size).clip(RoundedCornerShape(50)).clickable(onClick = onClick, role = androidx.compose.ui.semantics.Role.Button, onClickLabel = desc).semantics { contentDescription = desc }, contentAlignment = align) { MorphGlyph(glyph, 24.dp, MaterialTheme.colorScheme.primary) }

/** The familiar icon that goes with a one-word button. */
/** These buttons show only their icon (the words stay as the screen reader label); every other button keeps its words. */
val ICON_ONLY = setOf("Cancel", "Close", "Not now", "Got it", "Done", "Save", "Back", "Copy", "Share", "Attach", "Tour", "Setup", "What changed", "Look again", "Refresh", "Check for updates", "Check again", "Remove", "Add", "Read", "Stop reading")
val ACTION_ICONS = mapOf("Share this chat" to "share", "See what changed" to "file", "Undo this run" to "history", "Delete this chat…" to "trash", "Share" to "share", "Talk" to "mic", "Options" to "sliders", "Done" to "check", "Remove" to "trash", "Add" to "plus", "Look again" to "sync", "Refresh" to "sync",
    "Check for updates" to "sync", "Check again" to "sync", "Test call" to "phone", "Cancel" to "x", "Close" to "x", "Not now" to "x", "Got it" to "check", "Back" to "back", "Tour" to "tour",
    "Write an email" to "mail", "Contact" to "mail", "Attach" to "clip", "Copy" to "copy", "Run now" to "play", "Read" to "volume", "Stop reading" to "stop", "Undo" to "history", "Undo this run" to "history", "Back up now" to "download", "Restore the last backup" to "sync", "Report" to "mail", "Details" to "info", "Share" to "share", "Pin to top" to "pin", "Unpin" to "pin", "Rename" to "edit", "Pin" to "lock", "Allow" to "check", "Deny" to "x", "Connect" to "plus", "Update" to "download",
    "Make invite link" to "share", "Share link" to "share", "Stop sharing" to "trash", "Join" to "people", "Enter the address myself" to "link", "What changed" to "history", "Restart" to "sync", "Delete" to "trash", "Delete this chat…" to "trash", "Revoke" to "x", "Look again" to "sync", "Show all" to "plus", "Remove step" to "trash", "Open folder" to "folder", "Set up a workflow" to "sliders", "Add to tasks" to "check", "Fix with Claude" to "tour", "Save" to "check", "Install" to "download", "Continue" to "play", "Setup" to "play", "Background access" to "clock", "Full-screen calls" to "phone")
