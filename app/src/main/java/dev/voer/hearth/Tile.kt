package dev.voer.hearth

import android.app.PendingIntent
import android.content.Intent
import android.graphics.drawable.Icon
import android.service.quicksettings.Tile
import android.service.quicksettings.TileService

class UsageTile : TileService() {
    override fun onStartListening() {
        val t = qsTile ?: return
        val u = Store.usage(this)
        val now = System.currentTimeMillis()
        val s = u?.session
        t.label = "Claude"
        t.icon = Icon.createWithResource(this, R.drawable.ic_stat)
        t.state = if (u == null) Tile.STATE_INACTIVE else Tile.STATE_ACTIVE
        t.subtitle = if (s == null) "Not signed in" else
            "${s.at(now).toInt()}%" + (s.resetsAt?.takeIf { it > now }?.let { " · ${dur(it - now)}" } ?: "")
        t.updateTile()
    }

    override fun onClick() {
        val i = Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        if (android.os.Build.VERSION.SDK_INT >= 34)
            startActivityAndCollapse(PendingIntent.getActivity(this, 0, i, PendingIntent.FLAG_IMMUTABLE))
        else @Suppress("DEPRECATION") startActivityAndCollapse(i)
    }
}
