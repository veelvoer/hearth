package dev.voer.hearth

import android.Manifest
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaPlayer
import android.media.MediaRecorder
import android.os.Build
import android.os.Bundle
import android.speech.RecognitionListener
import android.speech.RecognizerIntent
import android.speech.SpeechRecognizer
import android.speech.tts.TextToSpeech
import android.speech.tts.UtteranceProgressListener
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.core.*
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import org.json.JSONObject
import java.io.ByteArrayOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.coroutines.resume
import kotlin.math.max
import kotlin.math.sqrt

/** Speech-to-text and text-to-speech served by the relay (faster-whisper and Piper on the computer). */
private fun relayError(c: java.net.HttpURLConnection, fallback: String) =
    runCatching { JSONObject(c.errorStream?.bufferedReader()?.readText().orEmpty()).optString("error") }.getOrNull()?.takeIf { it.isNotBlank() } ?: fallback

object VoiceApi {
    fun status(m: Machine): Pair<Boolean, Boolean> {
        val c = Relay.open(m, "/voice/status")
        try {
            val o = JSONObject(c.inputStream.bufferedReader().readText())
            return o.optBoolean("stt") to o.optBoolean("tts")
        } finally { c.disconnect() }
    }

    fun stt(m: Machine, wav: ByteArray): String {
        val c = Relay.open(m, "/voice/stt", post = true, read = 90_000)
        try {
            c.setRequestProperty("Content-Type", "audio/wav")
            c.outputStream.use { it.write(wav) }
            if (c.responseCode != 200) throw ApiException(c.responseCode, relayError(c, "Speech recognition failed"))
            return JSONObject(c.inputStream.bufferedReader().readText()).optString("text")
        } finally { c.disconnect() }
    }

    fun tts(m: Machine, text: String, dir: File, lang: String = "en"): File {
        val c = Relay.open(m, "/voice/tts", post = true, read = 60_000)
        try {
            c.outputStream.use { it.write(JSONObject().put("text", text).put("lang", lang).toString().toByteArray()) }
            if (c.responseCode != 200) throw ApiException(c.responseCode, relayError(c, "Speech synthesis failed"))
            return File.createTempFile("tts", ".wav", dir).also { f -> c.inputStream.use { i -> f.outputStream().use { o -> i.copyTo(o) } } }
        } finally { c.disconnect() }
    }
}

internal fun wav(pcm: ByteArray, rate: Int = 16_000): ByteArray {
    val b = ByteBuffer.allocate(44 + pcm.size).order(ByteOrder.LITTLE_ENDIAN)
    b.put("RIFF".toByteArray()).putInt(36 + pcm.size).put("WAVEfmt ".toByteArray()).putInt(16).putShort(1).putShort(1)
        .putInt(rate).putInt(rate * 2).putShort(2).putShort(16).put("data".toByteArray()).putInt(pcm.size).put(pcm)
    return b.array()
}

private val NL_WORDS = setOf("de", "het", "een", "en", "van", "ik", "je", "jij", "niet", "dat", "die", "op", "te", "met", "voor", "dit", "maar", "ook", "aan", "zijn", "heb", "hebben", "wat", "als", "naar", "bij", "kan", "wil", "nog", "uit", "dan", "er", "we", "wij", "ze", "hoe", "waar", "gaan", "maken", "hallo", "goed", "alles", "klaar", "gedaan", "bestand", "fout", "juist", "geen", "wordt", "deze", "heeft", "moet", "zou", "jouw", "mijn", "omdat", "want", "ja", "nee", "dank", "alsjeblieft")
private val EN_WORDS = setOf("the", "and", "of", "to", "that", "for", "with", "you", "it", "this", "are", "was", "have", "can", "will", "not", "but", "on", "in", "as", "be", "do", "what", "how", "we", "they", "from", "at", "your", "my", "hello", "done", "file", "error", "yes", "no", "thanks", "please", "should", "would", "which", "there", "been")

/** "nl" or "en", guessed from the words in [s]; [fallback] when it isn't clear (short or mixed text). */
fun detectLang(s: String, fallback: String): String {
    val words = s.lowercase().split(Regex("[^a-zà-ÿ']+")).filter { it.isNotEmpty() }
    val nl = words.count { it in NL_WORDS }; val en = words.count { it in EN_WORDS }
    return when { nl - en >= 2 -> "nl"; en - nl >= 2 -> "en"; else -> fallback }
}

fun localeFor(lang: String): java.util.Locale = if (lang == "nl") java.util.Locale("nl", "BE") else java.util.Locale.US

sealed interface Heard {
    data class Text(val t: String) : Heard
    data object Silence : Heard                      // nobody spoke: perfectly normal, keep listening
    data class Failed(val code: Int) : Heard         // the recognizer itself had a problem
}

fun recognizerProblem(code: Int) = when (code) {
    SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS -> "Hearth needs microphone permission."
    SpeechRecognizer.ERROR_NETWORK, SpeechRecognizer.ERROR_NETWORK_TIMEOUT, SpeechRecognizer.ERROR_SERVER, 11 -> "Speech recognition can't reach Google. Check your connection."
    SpeechRecognizer.ERROR_LANGUAGE_NOT_SUPPORTED, 13 -> "This language isn't available for speech recognition on your phone. Install it in the Google app settings."
    SpeechRecognizer.ERROR_AUDIO -> "The microphone is busy with another app."
    else -> "Speech recognition had a problem (code $code)."
}

interface Ears { suspend fun listen(level: (Float) -> Unit): Heard }


/** Android's own recognizer, on-device when the phone supports it. */
class PhoneEars(private val ctx: Context) : Ears {
    override suspend fun listen(level: (Float) -> Unit): Heard = withContext(Dispatchers.Main) {
        suspendCancellableCoroutine { cont ->
            val r = SpeechRecognizer.createSpeechRecognizer(ctx)
            var best = ""
            fun done(h: Heard) { if (cont.isActive) cont.resume(h); runCatching { r.destroy() } }
            r.setRecognitionListener(object : RecognitionListener {
                override fun onResults(b: Bundle?) {
                    val t = b?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull() ?: best
                    done(if (t.isBlank()) Heard.Silence else Heard.Text(t))
                }
                override fun onError(e: Int) {
                    when {
                        best.isNotBlank() -> done(Heard.Text(best))   // it heard something before giving up: use it
                        e == SpeechRecognizer.ERROR_NO_MATCH || e == SpeechRecognizer.ERROR_SPEECH_TIMEOUT -> done(Heard.Silence)
                        else -> done(Heard.Failed(e))
                    }
                }
                override fun onRmsChanged(v: Float) = level(((v + 2) / 12).coerceIn(0f, 1f))
                override fun onPartialResults(b: Bundle?) { b?.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION)?.firstOrNull()?.let { best = it } }
                override fun onReadyForSpeech(p: Bundle?) {}
                override fun onBeginningOfSpeech() {}
                override fun onBufferReceived(b: ByteArray?) {}
                override fun onEndOfSpeech() {}
                override fun onEvent(t: Int, b: Bundle?) {}
            })
            val tag = localeFor(CallSession.lang).toLanguageTag()
            r.startListening(Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH)
                .putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL, RecognizerIntent.LANGUAGE_MODEL_FREE_FORM)
                .putExtra(RecognizerIntent.EXTRA_LANGUAGE, tag)
                .putExtra(RecognizerIntent.EXTRA_LANGUAGE_PREFERENCE, tag)
                .putExtra(RecognizerIntent.EXTRA_PARTIAL_RESULTS, true)
                .putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1)
                .putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_COMPLETE_SILENCE_LENGTH_MILLIS, 1600L)
                .putExtra(RecognizerIntent.EXTRA_SPEECH_INPUT_POSSIBLY_COMPLETE_SILENCE_LENGTH_MILLIS, 1600L))
            cont.invokeOnCancellation { runCatching { r.cancel(); r.destroy() } }
        }
    }
}

interface Mouth {
    suspend fun prepare(text: String): Any?
    suspend fun play(item: Any?)
    fun stop()
}

class LaptopMouth(private val ctx: Context, private val m: Machine) : Mouth {
    private var mp: MediaPlayer? = null
    override suspend fun prepare(text: String): Any? = withContext(Dispatchers.IO) { runCatching { VoiceApi.tts(m, text, ctx.cacheDir, detectLang(text, CallSession.lang)) }.getOrNull() }
    override suspend fun play(item: Any?) {
        val f = item as? File ?: return
        try {
            suspendCancellableCoroutine { cont ->
                val p = MediaPlayer()
                mp = p
                p.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
                p.setDataSource(f.path)
                p.setOnCompletionListener { if (cont.isActive) cont.resume(Unit) }
                p.setOnErrorListener { _, _, _ -> if (cont.isActive) cont.resume(Unit); true }
                p.prepare(); p.start()
                cont.invokeOnCancellation { runCatching { p.stop() } }
            }
        } finally { mp?.release(); mp = null; f.delete() }
    }
    override fun stop() { runCatching { mp?.stop() } }
}

class PhoneMouth(ctx: Context) : Mouth {
    private val ready = CompletableDeferred<Boolean>()
    private val waiting = HashMap<String, CancellableContinuation<Unit>>()
    private var n = 0
    private val tts: TextToSpeech = TextToSpeech(ctx.applicationContext) { ready.complete(it == TextToSpeech.SUCCESS) }

    init {
        tts.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
        tts.setOnUtteranceProgressListener(object : UtteranceProgressListener() {
            override fun onStart(id: String?) {}
            override fun onDone(id: String?) { synchronized(waiting) { waiting.remove(id) }?.let { if (it.isActive) it.resume(Unit) } }
            @Deprecated("Deprecated in Java") override fun onError(id: String?) = onDone(id)
        })
    }

    override suspend fun prepare(text: String): Any? = text
    override suspend fun play(item: Any?) {
        val t = item as? String ?: return
        if (!ready.await()) return
        val lang = detectLang(t, CallSession.lang)
        val r = tts.setLanguage(localeFor(lang))
        if (r == TextToSpeech.LANG_MISSING_DATA || r == TextToSpeech.LANG_NOT_SUPPORTED) tts.setLanguage(java.util.Locale.getDefault())
        suspendCancellableCoroutine<Unit> { cont ->
            val id = "u${n++}"
            synchronized(waiting) { waiting[id] = cont }
            cont.invokeOnCancellation { tts.stop() }
            tts.speak(t, TextToSpeech.QUEUE_FLUSH, null, id)
        }
    }
    override fun stop() { tts.stop() }
    fun shutdown() { tts.shutdown() }
}

/** Plays sentences in order while the next ones are still being synthesised. */
class Speaker(private val scope: CoroutineScope, private val mouth: Mouth) {
    private var chan = Channel<Deferred<Any?>>(Channel.UNLIMITED)
    private var job: Job? = null
    @Volatile private var pending = 0
    val busy get() = pending > 0

    init { start() }

    private fun start() {
        val ch = chan
        job = scope.launch {
            for (d in ch) { runCatching { mouth.play(d.await()) }; pending-- }
        }
    }

    fun enqueue(text: String) {
        if (text.isBlank()) return
        pending++
        chan.trySend(scope.async(Dispatchers.IO) { mouth.prepare(text) })
    }

    suspend fun drain() { while (pending > 0) delay(80) }

    fun reset() {
        job?.cancel(); mouth.stop(); chan.close(); pending = 0
        chan = Channel(Channel.UNLIMITED); start()
    }
}

/** Splits streamed text into speakable sentences; the unfinished tail stays in [buf]. */
fun takeSentences(buf: StringBuilder): List<String> {
    val out = ArrayList<String>()
    while (true) {
        val m = Regex("([.!?]+[\"')\\]]*\\s+|\\n+)").find(buf) ?: break
        val end = m.range.last + 1
        if (end >= buf.length && !m.value.contains('\n')) break  // might be "3." of "3.5"; wait for more text
        out += buf.substring(0, end).trim()
        buf.delete(0, end)
    }
    return out.filter { it.isNotBlank() }
}

fun spoken(s: String) = s.replace(Regex("```[\\s\\S]*?```"), " code omitted. ").replace(Regex("(?m)^\\s*\\|.*$"), "").replace(Regex("[*_`#>]+"), "")
    .replace(Regex("https?://\\S+"), "a link").trim()


enum class Phase { Connecting, Listening, Thinking, Speaking }
