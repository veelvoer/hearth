package dev.voer.hearth.wear

import android.content.Context
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.setValue
import com.google.android.gms.wearable.DataEvent
import com.google.android.gms.wearable.DataEventBuffer
import com.google.android.gms.wearable.DataMapItem
import com.google.android.gms.wearable.Wearable
import com.google.android.gms.wearable.WearableListenerService
import kotlinx.coroutines.tasks.await
import org.json.JSONArray

/** A Hearth server or computer this watch talks to. The phone hands them over; nothing is typed on the watch. */
data class Server(val name: String, val base: String, val token: String)

object Servers {
    const val PATH = "/hearth/machines"
    var rev by mutableIntStateOf(0)
    private fun sp(c: Context) = c.getSharedPreferences("watch", Context.MODE_PRIVATE)

    fun all(c: Context): List<Server> = runCatching {
        val a = JSONArray(sp(c).getString("machines", "[]"))
        (0 until a.length()).map { a.getJSONObject(it) }.filter { it.optString("base").startsWith("http") && it.optString("token").isNotEmpty() }
            .map { Server(it.optString("name", "Server"), it.optString("base").trimEnd('/'), it.optString("token")) }
    }.getOrDefault(emptyList())

    fun save(c: Context, json: String) {
        if (runCatching { JSONArray(json) }.isFailure) return
        if (sp(c).getString("machines", null) == json) return
        sp(c).edit().putString("machines", json).apply(); rev++
    }

    /** What the phone last put in the data layer: read when the app opens, in case the update came while it was closed. */
    suspend fun fetchFromPhone(c: Context) {
        runCatching {
            val items = Wearable.getDataClient(c).dataItems.await()
            try { items.forEach { if (it.uri.path == PATH) DataMapItem.fromDataItem(it).dataMap.getString("json")?.let { j -> save(c, j) } } } finally { items.release() }
        }
    }
}

/** Gets the list of servers from the phone app whenever it changes. */
class PhoneLink : WearableListenerService() {
    override fun onDataChanged(events: DataEventBuffer) {
        events.forEach { e -> if (e.type == DataEvent.TYPE_CHANGED && e.dataItem.uri.path == Servers.PATH) DataMapItem.fromDataItem(e.dataItem).dataMap.getString("json")?.let { Servers.save(this, it) } }
    }
}
