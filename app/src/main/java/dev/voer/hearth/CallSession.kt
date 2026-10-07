package dev.voer.hearth

import android.app.*
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.media.AudioDeviceInfo
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.MediaRecorder
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.compose.runtime.*
import androidx.core.app.NotificationCompat
import androidx.core.app.Person
import androidx.core.app.ServiceCompat
import androidx.core.content.ContextCompat
import android.speech.SpeechRecognizer
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import java.io.ByteArrayOutputStream
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.max
import kotlin.math.sqrt

/** Routes call audio like a phone call: earpiece by default, speaker on request, screen off at your ear. */
class CallAudio(c: Context) {
    private val am = c.getSystemService(AudioManager::class.java)
    private val pm = c.getSystemService(PowerManager::class.java)
    private var prox: PowerManager.WakeLock? = null
    private var prevMode = AudioManager.MODE_NORMAL

    fun start() { prevMode = am.mode; am.mode = AudioManager.MODE_IN_COMMUNICATION; route(false) }

    fun route(speaker: Boolean) {
        if (Build.VERSION.SDK_INT >= 31) {
            val devs = am.availableCommunicationDevices
            val pick = if (speaker) devs.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
            else listOf(AudioDeviceInfo.TYPE_WIRED_HEADSET, AudioDeviceInfo.TYPE_USB_HEADSET, AudioDeviceInfo.TYPE_BLE_HEADSET,
                AudioDeviceInfo.TYPE_BLUETOOTH_SCO, AudioDeviceInfo.TYPE_BUILTIN_EARPIECE).firstNotNullOfOrNull { t -> devs.firstOrNull { it.type == t } }
            if (pick != null) runCatching { am.setCommunicationDevice(pick) }
        } else @Suppress("DEPRECATION") { am.isSpeakerphoneOn = speaker }
        proximity(!speaker)
    }

    @Suppress("WakelockTimeout")
    private fun proximity(on: Boolean) {
        if (on) {
            if (prox == null && pm.isWakeLockLevelSupported(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK))
                prox = pm.newWakeLock(PowerManager.PROXIMITY_SCREEN_OFF_WAKE_LOCK, "hearth:call").also { it.acquire(3 * 3600_000L) }
        } else { prox?.let { if (it.isHeld) it.release() }; prox = null }
    }

    fun stop() {
        proximity(false)
        if (Build.VERSION.SDK_INT >= 31) am.clearCommunicationDevice() else @Suppress("DEPRECATION") { am.isSpeakerphoneOn = false }
        am.mode = prevMode
    }
}

/** Always-on microphone with voice-activity detection, so you can talk and interrupt like in a normal call. */
class DuplexEars {
    suspend fun run(speaking: () -> Boolean, speakerOn: () -> Boolean, muted: () -> Boolean, level: (Float) -> Unit,
                    onBarge: () -> Unit, onUtterance: (ByteArray) -> Unit) = withContext(Dispatchers.IO) {
        val rate = 16_000; val frame = 320
        val rec = AudioRecord(MediaRecorder.AudioSource.VOICE_COMMUNICATION, rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT,
            max(AudioRecord.getMinBufferSize(rate, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT), 8192))
        val pre = ArrayDeque<ShortArray>()
        var seg: ArrayList<ShortArray>? = null
        var floor = 0.0; var i = 0; var run = 0; var lastLoud = 0; var segStart = 0
        try {
            rec.startRecording()
            val buf = ShortArray(frame)
            while (isActive) {
                var n = 0
                while (n < frame) { val r = rec.read(buf, n, frame - n); if (r <= 0) break; n += r }
                if (n < frame) break
                i++
                if (muted()) { seg = null; pre.clear(); run = 0; level(0f); continue }
                val rms = sqrt(buf.fold(0.0) { a, s -> a + s * s.toDouble() } / frame)
                level((rms / 4000).toFloat().coerceIn(0f, 1f))
                if (i <= 15) floor = max(floor, rms) else if (seg == null && rms < floor * 2) floor = floor * .98 + rms * .02
                val sp = speaking(); val spk = speakerOn()
                // while Claude talks the bar is higher (echo), so only a real voice interrupts
                val thr = if (sp) max(floor * if (spk) 6.0 else 4.0, if (spk) 2500.0 else 1800.0) else max(floor * 2.5, 700.0)
                val need = if (sp) (if (spk) 12 else 8) else 3
                val frameCopy = buf.copyOf()
                val cur = seg
                if (cur == null) {
                    pre.addLast(frameCopy); if (pre.size > 15) pre.removeFirst()
                    run = if (rms > thr) run + 1 else 0
                    if (run >= need) { seg = ArrayList(pre); lastLoud = i; segStart = i; run = 0; if (sp) onBarge() }
                } else {
                    cur.add(frameCopy)
                    if (rms > max(floor * 2.5, 700.0)) lastLoud = i
                    if (i - lastLoud > 45 || i - segStart > 1500) {
                        if (lastLoud - segStart > 12) {
                            val out = ByteArrayOutputStream()
                            cur.forEach { f -> f.forEach { out.write(it.toInt() and 0xFF); out.write((it.toInt() shr 8) and 0xFF) } }
                            onUtterance(out.toByteArray())
                        }
                        seg = null; pre.clear()
                    }
                }
            }
        } finally { runCatching { rec.stop() }; rec.release() }
    }
}

/** One live voice call with Claude. Lives outside the UI so it survives the screen turning off or you switching apps. */
object CallSession {
    var active by mutableStateOf(false)
    var phase by mutableStateOf(Phase.Connecting)
    var level by mutableFloatStateOf(0f)
    var muted by mutableStateOf(false)
    var speakerOn by mutableStateOf(false)
    var captions by mutableStateOf(true)
    var error by mutableStateOf<String?>(null)
    var status by mutableStateOf("")
    var lang by mutableStateOf("en")
    var startedAt = 0L
    val lines = mutableStateListOf<Pair<Boolean, String>>()
    var host = ""; var sessId = ""; var title = ""; private var mode = "acceptEdits"

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    private var job: Job? = null
    private var audio: CallAudio? = null
    private var talker: Speaker? = null
    private var mouth: Mouth? = null
    private var app: Context? = null
    private val drop = AtomicBoolean(false)

    fun start(c: Context, m: Machine, s: Sess, mode: String) {
        if (active) return
        val a = c.applicationContext
        app = a; host = m.host; sessId = s.id; title = s.title; this.mode = mode
        lines.clear(); error = null; status = ""; muted = false; speakerOn = false; captions = true
        lang = Store.voiceLang(a)
        phase = Phase.Connecting; startedAt = System.currentTimeMillis(); active = true
        ContextCompat.startForegroundService(a, Intent(a, CallAudioService::class.java))
        audio = CallAudio(a).also { it.start() }
        job = scope.launch {
            val pref = Store.voiceEngine(a)
            val (stt, tts) = if (pref == "phone") false to false else withContext(Dispatchers.IO) { runCatching { VoiceApi.status(m) }.getOrDefault(false to false) }
            val mo: Mouth = if (tts) LaptopMouth(a, m) else PhoneMouth(a)
            mouth = mo
            val sp = Speaker(scope, mo); talker = sp
            phase = Phase.Listening
            runCatching { if (stt) duplex(m, s.id, sp) else turnBased(a, m, s.id, sp) }.onFailure { if (it !is CancellationException) error = it.message }
        }
    }

    fun toggleSpeaker() { speakerOn = !speakerOn; audio?.route(speakerOn) }

    fun end() {
        if (!active) return
        active = false
        job?.cancel(); talker?.reset(); (mouth as? PhoneMouth)?.shutdown()
        audio?.stop(); audio = null; talker = null; mouth = null
        app?.let { it.stopService(Intent(it, CallAudioService::class.java)) }
        level = 0f
    }

    private suspend fun duplex(m: Machine, sid: String, sp: Speaker) = coroutineScope {
        val texts = Channel<String>(Channel.UNLIMITED)
        launch { for (t in texts) { lines += true to t; turn(m, sid, t, sp) } }
        DuplexEars().run({ sp.busy }, { speakerOn }, { muted }, { level = it }, { drop.set(true); sp.reset(); if (phase == Phase.Speaking) phase = Phase.Listening }) { pcm ->
            launch(Dispatchers.IO) {
                val text = runCatching { VoiceApi.stt(m, wav(pcm)) }.getOrElse { error = it.message ?: "Speech recognition failed"; "" }
                if (text.isNotBlank()) { error = null; texts.trySend(text.trim()) }
            }
        }
    }

    private suspend fun turnBased(a: Context, m: Machine, sid: String, sp: Speaker) {
        var failures = 0
        while (currentCoroutineContext().isActive) {
            if (muted) { delay(250); continue }
            phase = Phase.Listening
            val heard = runCatching { PhoneEars(a).listen { level = it } }.getOrElse { Heard.Failed(-1) }
            when (heard) {
                is Heard.Silence -> { failures = 0; error = null; continue }       // quiet moment: just keep listening, like a phone call
                is Heard.Failed -> {
                    error = recognizerProblem(heard.code)
                    failures++
                    delay(if (heard.code == SpeechRecognizer.ERROR_RECOGNIZER_BUSY) 600L else (800L * failures).coerceAtMost(4_000L))
                    continue
                }
                is Heard.Text -> {
                    failures = 0; error = null
                    lines += true to heard.t
                    if (!turn(m, sid, heard.t, sp)) return
                }
            }
        }
    }

    private suspend fun turn(m: Machine, sid: String, text: String, sp: Speaker): Boolean {
        phase = Phase.Thinking; status = ""; drop.set(false); sp.reset()
        val buf = StringBuilder(); val reply = StringBuilder()
        val ok = runCatching {
            withContext(Dispatchers.IO) {
                Relay.send(m, sid, text, mode, brief = app?.let { Store.voiceBrief(it) } ?: false, model = "haiku") { t, tx, _ ->
                    when (t) {
                        "delta" -> { reply.append(tx); buf.append(tx)
                            takeSentences(buf).forEach { if (!drop.get()) { phase = Phase.Speaking; sp.enqueue(spoken(it)) } } }
                        "tool" -> status = "Working: $tx"
                    }
                }
            }
        }.onFailure { error = it.message ?: "Connection lost" }.isSuccess
        if (ok && buf.isNotBlank() && !drop.get()) { phase = Phase.Speaking; sp.enqueue(spoken(buf.toString())) }
        if (reply.isNotBlank()) lines += false to reply.toString().trim()
        sp.drain()
        status = ""; phase = Phase.Listening
        return ok
    }
}

/** Keeps the microphone alive when the screen turns off at your ear, and shows the ongoing call. */
class CallAudioService : Service() {
    override fun onBind(i: Intent?): IBinder? = null

    override fun onStartCommand(i: Intent?, f: Int, id: Int): Int {
        if (i?.action == "end") { CallSession.end(); stopSelf(); return START_NOT_STICKY }
        val nm = getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel("ongoing", "Ongoing call", NotificationManager.IMPORTANCE_LOW))
        val fl = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        val open = PendingIntent.getActivity(this, 5, Intent(this, MainActivity::class.java).putExtra("open_host", CallSession.host)
            .putExtra("open_session", CallSession.sessId).putExtra("open_title", CallSession.title).putExtra("open_voice", true)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP), fl)
        val hangUp = PendingIntent.getService(this, 6, Intent(this, CallAudioService::class.java).setAction("end"), fl)
        val n = NotificationCompat.Builder(this, "ongoing").setSmallIcon(R.drawable.ic_stat).setContentTitle("Call with Claude")
            .setContentText(CallSession.title).setOngoing(true).setUsesChronometer(true).setWhen(CallSession.startedAt)
            .setCategory(NotificationCompat.CATEGORY_CALL).setContentIntent(open)
            .setStyle(NotificationCompat.CallStyle.forOngoingCall(Person.Builder().setName("Claude").setImportant(true).build(), hangUp)).build()
        ServiceCompat.startForeground(this, 2, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
        return START_NOT_STICKY
    }
}
