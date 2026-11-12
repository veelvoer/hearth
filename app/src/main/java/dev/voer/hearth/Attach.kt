package dev.voer.hearth

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.unit.dp

/** Small previews of the pictures you attached, kept for this app session and keyed by their path on the computer. */
object Thumbs {
    val map = HashMap<String, android.graphics.Bitmap>()
    fun make(bytes: ByteArray): android.graphics.Bitmap? = runCatching {
        val o = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
        android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, o)
        var s = 1; while (o.outWidth / s > 480 || o.outHeight / s > 480) s *= 2
        android.graphics.BitmapFactory.decodeByteArray(bytes, 0, bytes.size, android.graphics.BitmapFactory.Options().apply { inSampleSize = s })
    }.getOrNull()
}

/** A prompt as it was sent, with the "@path" lines of attached files taken out: the text and the paths. */
fun splitAttachments(text: String): Pair<String, List<String>> {
    val lines = text.lines(); val paths = ArrayList<String>(); val rest = ArrayList<String>()
    for (l in lines) { val t = l.trim(); if (t.startsWith("@/") && !t.contains(' ')) paths.add(t.removePrefix("@")) else rest.add(l) }
    return rest.joinToString("\n").trim() to paths
}

@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
/** What was attached to a prompt: a preview for pictures sent from this phone, otherwise the file name. */
@Composable
fun AttachedRow(paths: List<String>) {
    if (paths.isEmpty()) return
    androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        paths.forEach { p ->
            val b = Thumbs.map[p]
            if (b != null) Image(b.asImageBitmap(), p.substringAfterLast('/'), Modifier.size(96.dp).clip(RoundedCornerShape(12.dp)), contentScale = ContentScale.Crop)
            else Row(Modifier.clip(RoundedCornerShape(10.dp)).background(softFill()).padding(horizontal = 10.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                Glyph("clip", 16.dp, androidx.compose.material3.MaterialTheme.colorScheme.onSurfaceVariant); Spacer(Modifier.width(6.dp)); Txt(p.substringAfterLast('/').takeLast(28), T.small)
            }
        }
    }
}
