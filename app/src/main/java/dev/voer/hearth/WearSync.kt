package dev.voer.hearth

import android.content.Context
import org.json.JSONArray
import org.json.JSONObject

/** Hands your servers to the Hearth watch app over the Wear OS data layer. Does nothing when there is no watch. */
object WearSync {
    private var last = ""
    fun push(c: Context) {
        runCatching {
            val a = JSONArray()
            Store.machines(c).filter { !it.guest }.forEach { a.put(JSONObject().put("name", it.name).put("base", it.base).put("token", it.token)) }
            val json = a.toString(); if (json == last) return; last = json
            val req = com.google.android.gms.wearable.PutDataMapRequest.create("/hearth/machines").apply { dataMap.putString("json", json) }.asPutDataRequest().setUrgent()
            com.google.android.gms.wearable.Wearable.getDataClient(c.applicationContext).putDataItem(req)
        }
    }
}
