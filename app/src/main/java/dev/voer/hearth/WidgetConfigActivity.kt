package dev.voer.hearth

import android.app.Activity
import android.appwidget.AppWidgetManager
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Switch
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

class WidgetConfigActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setResult(Activity.RESULT_CANCELED)
        val id = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, 0)
        val m = AppWidgetManager.getInstance(this)
        var shape = if (id != 0) when (m.getAppWidgetInfo(id)?.provider?.className?.substringAfterLast('.')) {
            "TinyWidget" -> Shape.TINY; "WideWidget" -> Shape.WIDE; "StripWidget" -> Shape.STRIP; else -> Shape.SMALL
        } else runCatching { Shape.valueOf(intent.getStringExtra("shape") ?: "") }.getOrDefault(Shape.SMALL)
        val u = Store.usage(this)
        setContent {
            ClaudeTheme {
                var cfg by remember { mutableStateOf(Store.widget(this, id)) }
                var sh by remember { mutableStateOf(shape) }
                Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).systemBarsPadding()) {
                    Screen {
                        Header("Customize widget", if (id == 0) "Default for new widgets" else null)
                        if (id == 0) Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            Shape.entries.forEach { s -> Chip(s.label, sh == s) { sh = s } }
                        }
                        Card { Preview(cfg, sh, u) }
                        Card {
                            Label("Style"); Spacer(Modifier.height(8.dp))
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Look.entries.forEach { l -> Chip(l.label, cfg.look == l) { cfg = cfg.copy(look = l) } } }
                            if (sh == Shape.TINY) {
                                Spacer(Modifier.height(16.dp)); Label("Mini content"); Spacer(Modifier.height(8.dp))
                                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    listOf("Ring + %", "Ring", "%").forEachIndexed { i, s -> Chip(s, cfg.tiny == i) { cfg = cfg.copy(tiny = i) } }
                                }
                            }
                            Spacer(Modifier.height(16.dp)); Label("Show"); Spacer(Modifier.height(8.dp))
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) { Show.entries.forEach { s -> Chip(s.label, cfg.show == s) { cfg = cfg.copy(show = s) } } }
                            Spacer(Modifier.height(16.dp)); Label("Value"); Spacer(Modifier.height(8.dp))
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Chip("Used", !cfg.remaining) { cfg = cfg.copy(remaining = false) }
                                Chip("Remaining", cfg.remaining) { cfg = cfg.copy(remaining = true) }
                            }
                            Spacer(Modifier.height(16.dp)); Label("Colors"); Spacer(Modifier.height(8.dp))
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                listOf("Auto", "Dark", "Light").forEachIndexed { i, s -> Chip(s, cfg.mode == i) { cfg = cfg.copy(mode = i) } }
                            }
                            Spacer(Modifier.height(16.dp)); Label("Accent"); Spacer(Modifier.height(8.dp))
                            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                                val sw = listOf(null, Color(0xFFF0643C), Color(0xFFD71921), Color(0xFF8C8C8C), Color(0xFF4CAF7D), Color(0xFF5B8DEF))
                                sw.forEachIndexed { i, col ->
                                    val sel = cfg.accent == i
                                    Box(Modifier.size(32.dp).clip(CircleShape)
                                        .background(col ?: Color.Transparent)
                                        .border(if (sel) 2.dp else 1.dp, if (sel) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.outline, CircleShape)
                                        .clickable { cfg = cfg.copy(accent = i) }, contentAlignment = Alignment.Center) {
                                        if (col == null) Txt("A", T.label, muted = true)
                                    }
                                }
                            }
                            Spacer(Modifier.height(16.dp)); Label("Background opacity"); 
                            Slider(cfg.bgAlpha / 100f, { cfg = cfg.copy(bgAlpha = (it * 100).toInt()) })
                            Label("Corner radius")
                            Slider(cfg.radius / 40f, { cfg = cfg.copy(radius = (it * 40).toInt()) })
                            Row2("Show reset time") { Switch(cfg.reset, { cfg = cfg.copy(reset = it) }) }
                        }
                        Button("Save", Modifier.fillMaxWidth()) {
                            Store.saveWidget(this@WidgetConfigActivity, id, cfg)
                            if (id != 0) {
                                setResult(Activity.RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, id))
                                WidgetUpdater.updateAll(this@WidgetConfigActivity)
                            }
                            finish()
                        }
                    }
                }
            }
        }
    }
}
