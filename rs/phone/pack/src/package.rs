//! hearth-pack: builds the Rust Hearth apps into signed debug APKs without Gradle.
//!
//!   cargo run -p hearth-pack -- phone            # dev.voer.hearth.rust
//!   cargo run -p hearth-pack -- watch            # dev.voer.hearth.rust.watch
//!   options: --abi arm64-v8a,x86_64   --debug-lib   --out <dir>   --skip-rust
//!
//! Steps: `cargo ndk` (the cdylib per ABI) -> generate manifest, resources and the few Java classes Android forces
//! -> aapt2 compile/link -> javac + d8 (own Java and the Google Play services client from the Gradle cache)
//! -> add classes.dex and lib/<abi>/*.so to the apk -> zipalign (16 KB pages) -> apksigner (debug keystore).
//! No Gradle, no Kotlin. Temp files live in $HEARTH_TMP (default ~/hearth-tmp), never /tmp.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

type R<T> = Result<T, String>;

const JAVA_PKG: &str = "dev.voer.hearth.rust";
const COMPILE_SDK: &str = "android-35";

struct App {
    kind: &'static str,
    app_id: &'static str,
    crate_name: &'static str,
    lib: &'static str,
    label: &'static str,
    min_sdk: u32,
    watch: bool,
}

const PHONE: App = App { kind: "phone", app_id: "dev.voer.hearth.rust", crate_name: "hearth-phone", lib: "hearth_phone", label: "Hearth (Rust)", min_sdk: 26, watch: false };
const WATCH: App = App { kind: "watch", app_id: "dev.voer.hearth.rust.watch", crate_name: "hearth-watch", lib: "hearth_watch", label: "Hearth (Rust)", min_sdk: 30, watch: true };

fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
}

fn env_path(k: &str, default: PathBuf) -> PathBuf {
    std::env::var(k).map(PathBuf::from).unwrap_or(default)
}

fn run(cmd: &mut Command) -> R<String> {
    eprintln!("+ {:?}", cmd);
    let out = cmd.output().map_err(|e| format!("{:?}: {e}", cmd.get_program()))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        return Err(format!("{:?} failed ({}):\n{}", cmd.get_program(), out.status, text));
    }
    Ok(text)
}

fn newest_subdir(dir: &Path) -> R<PathBuf> {
    let mut v: Vec<PathBuf> = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    v.sort_by_key(|p| p.file_name().map(|n| version_key(&n.to_string_lossy())).unwrap_or_default());
    v.pop().ok_or_else(|| format!("nothing in {}", dir.display()))
}

fn version_key(s: &str) -> Vec<u32> {
    s.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect()
}

struct Tools {
    sdk: PathBuf,
    bt: PathBuf,
    android_jar: PathBuf,
    javac: PathBuf,
    java_home: Option<PathBuf>,
    root: PathBuf,
    tmp: PathBuf,
}

impl Tools {
    fn find() -> R<Tools> {
        let sdk = env_path("ANDROID_HOME", home().join("Android/sdk"));
        let bt = newest_subdir(&sdk.join("build-tools"))?;
        let android_jar = sdk.join("platforms").join(COMPILE_SDK).join("android.jar");
        if !android_jar.exists() {
            return Err(format!("missing {}", android_jar.display()));
        }
        let mut cands: Vec<PathBuf> = vec![];
        if let Ok(j) = std::env::var("JAVA_HOME") {
            cands.push(PathBuf::from(j));
        }
        cands.push(home().join("jdk"));
        if let Ok(d) = fs::read_dir(home().join(".local/jdk")) {
            cands.extend(d.filter_map(|e| e.ok()).map(|e| e.path()));
        }
        let java_home = cands.into_iter().find(|c| c.join("bin/javac").exists());
        let javac = java_home.as_ref().map(|j| j.join("bin/javac")).ok_or("no JDK with javac found (set JAVA_HOME)")?;
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().map_err(|e| e.to_string())?;
        let tmp = env_path("HEARTH_TMP", home().join("hearth-tmp")).join("pack");
        Ok(Tools { sdk, bt, android_jar, javac, java_home, root, tmp })
    }
    fn d8(&self) -> Command {
        let mut c = Command::new(self.bt.join("d8"));
        if let Some(j) = &self.java_home {
            c.env("JAVA_HOME", j);
        }
        c
    }
    fn apksigner(&self) -> Command {
        let mut c = Command::new(self.bt.join("apksigner"));
        if let Some(j) = &self.java_home {
            c.env("JAVA_HOME", j);
        }
        c
    }
    fn ndk(&self) -> R<PathBuf> {
        if let Ok(n) = std::env::var("ANDROID_NDK_HOME") {
            return Ok(PathBuf::from(n));
        }
        newest_subdir(&self.sdk.join("ndk"))
    }
}

fn workspace_version(root: &Path) -> String {
    let t = fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    t.lines().find(|l| l.trim_start().starts_with("version")).and_then(|l| l.split('"').nth(1)).unwrap_or("0.0.0").to_string()
}

fn version_code(v: &str) -> u32 {
    let k = version_key(v);
    // 1.40.0 -> 1_040_000 + 1 so it is always apart from the Kotlin app's numbers
    k.first().copied().unwrap_or(0) * 1_000_000 + k.get(1).copied().unwrap_or(0) * 1_000 + k.get(2).copied().unwrap_or(0)
}

// ---------- generated files ----------

fn manifest(app: &App, version: &str) -> String {
    let mut m = String::new();
    m.push_str(&format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="{id}" android:versionCode="{code}" android:versionName="{ver}">
    <uses-sdk android:minSdkVersion="{min}" android:targetSdkVersion="35" />
    <uses-permission android:name="android.permission.INTERNET" />
    <uses-permission android:name="android.permission.WAKE_LOCK" />
    <uses-permission android:name="android.permission.VIBRATE" />
"#,
        id = app.app_id,
        code = version_code(version),
        ver = version,
        min = app.min_sdk
    ));
    if app.watch {
        m.push_str("    <uses-feature android:name=\"android.hardware.type.watch\" />\n");
    } else {
        m.push_str(
            r#"    <uses-permission android:name="android.permission.POST_NOTIFICATIONS" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE" />
    <uses-permission android:name="android.permission.FOREGROUND_SERVICE_MICROPHONE" />
    <uses-permission android:name="android.permission.RECORD_AUDIO" />
"#,
        );
    }
    m.push_str(
        r#"    <queries>
        <intent><action android:name="android.speech.action.RECOGNIZE_SPEECH" /></intent>
        <intent><action android:name="android.media.action.IMAGE_CAPTURE" /></intent>
    </queries>
"#,
    );
    m.push_str(&format!(
        r#"    <application
        android:label="{label}"
        android:icon="@mipmap/ic_launcher"
        android:allowBackup="false"
        android:hasCode="true"
        android:usesCleartextTraffic="true"
        android:theme="@android:style/Theme.DeviceDefault.NoActionBar">
"#,
        label = app.label
    ));
    if app.watch {
        m.push_str(
            r#"        <uses-library android:name="com.google.android.wearable" android:required="false" />
        <meta-data android:name="com.google.android.wearable.standalone" android:value="false" />
"#,
        );
    }
    // GmsCore checks this tag before it hands out the Wearable client (12451000 = the Play services client version)
    m.push_str("        <meta-data android:name=\"com.google.android.gms.version\" android:value=\"12451000\" />\n");
    m.push_str(&format!(
        r#"        <activity
            android:name="{pkg}.HearthActivity"
            android:exported="true"
            android:configChanges="orientation|screenSize|screenLayout|keyboardHidden|keyboard|navigation|uiMode|smallestScreenSize|density|locale|layoutDirection"
            android:windowSoftInputMode="adjustResize"
            android:launchMode="singleTask">
            <meta-data android:name="android.app.lib_name" android:value="{lib}" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
"#,
        pkg = JAVA_PKG,
        lib = app.lib
    ));
    if !app.watch {
        m.push_str(
            r#"            <intent-filter>
                <action android:name="android.intent.action.VIEW" />
                <category android:name="android.intent.category.DEFAULT" />
                <category android:name="android.intent.category.BROWSABLE" />
                <data android:scheme="hearth" />
            </intent-filter>
"#,
        );
    }
    m.push_str("        </activity>\n");
    if !app.watch {
        m.push_str(&format!(
            r#"        <service android:name="{pkg}.CallService" android:exported="false" android:foregroundServiceType="microphone" />
        <!-- the watch asks its questions here over Bluetooth -->
        <service android:name="{pkg}.WatchLink" android:exported="true">
            <intent-filter>
                <action android:name="com.google.android.gms.wearable.REQUEST_RECEIVED" />
                <data android:scheme="wear" android:host="*" android:pathPrefix="/hearth/rpc" />
            </intent-filter>
        </service>
"#,
            pkg = JAVA_PKG
        ));
    }
    m.push_str("    </application>\n</manifest>\n");
    m
}

fn resources(app: &App, res: &Path) -> R<()> {
    let w = |p: &str, c: &str| -> R<()> {
        let f = res.join(p);
        fs::create_dir_all(f.parent().unwrap()).map_err(|e| e.to_string())?;
        fs::write(f, c).map_err(|e| e.to_string())
    };
    // one hearth-shaped glyph (heart-like flame) used as notification icon and launcher foreground
    let glyph = "M12,21 C12,21 4,15.5 4,9.5 C4,6.5 6.2,4.5 8.6,4.5 C10.1,4.5 11.3,5.3 12,6.5 C12.7,5.3 13.9,4.5 15.4,4.5 C17.8,4.5 20,6.5 20,9.5 C20,15.5 12,21 12,21 Z";
    w("drawable/ic_stat.xml", &format!(r##"<vector xmlns:android="http://schemas.android.com/apk/res/android" android:width="24dp" android:height="24dp" android:viewportWidth="24" android:viewportHeight="24"><path android:fillColor="#FFFFFFFF" android:pathData="{glyph}"/></vector>"##))?;
    w("drawable/ic_launcher_fg.xml", &format!(r##"<vector xmlns:android="http://schemas.android.com/apk/res/android" android:width="108dp" android:height="108dp" android:viewportWidth="24" android:viewportHeight="24"><group android:scaleX="0.6" android:scaleY="0.6" android:pivotX="12" android:pivotY="12"><path android:fillColor="#FFFFFFFF" android:pathData="{glyph}"/></group></vector>"##))?;
    w("values/colors.xml", r#"<resources><color name="ic_bg">#C4572F</color></resources>"#)?;
    w("mipmap-anydpi-v26/ic_launcher.xml", r#"<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android"><background android:drawable="@color/ic_bg"/><foreground android:drawable="@drawable/ic_launcher_fg"/></adaptive-icon>"#)?;
    if !app.watch {
        // lets watches find this phone (CapabilityClient)
        w("values/wear.xml", "<resources><string-array name=\"android_wear_capabilities\"><item>hearth_phone_link</item></string-array></resources>")?;
    }
    Ok(())
}

/// The Java Android forces, one entry per class. Kept as small as possible; hearth-pack lists them at the end.
fn java_sources(app: &App) -> Vec<(&'static str, &'static str, String)> {
    let mut v: Vec<(&str, &str, String)> = vec![];
    v.push((
        "HearthNative",
        "native method declarations; the bodies are Rust (export_natives!)",
        format!(
            r#"package {JAVA_PKG};

final class HearthNative {{
    static {{ System.loadLibrary("{lib}"); }}
    static native void onResult(int req, boolean ok, String data);
    static native void onPermission(String perm, boolean granted);
    static native byte[] watchRequest(String filesDir, byte[] request);
}}
"#,
            lib = app.lib
        ),
    ));
    v.push((
        "HearthActivity",
        "NativeActivity gives Rust no hook for onActivityResult / onRequestPermissionsResult (file picker, camera, speech, permissions)",
        format!(
            r#"package {JAVA_PKG};

import android.content.Intent;
import java.util.ArrayList;

public class HearthActivity extends android.app.NativeActivity {{
    @Override protected void onActivityResult(int req, int res, Intent d) {{
        super.onActivityResult(req, res, d);
        String s = null;
        if (d != null) {{
            if (d.getData() != null) s = d.getData().toString();
            else {{
                ArrayList<String> r = d.getStringArrayListExtra("android.speech.extra.RESULTS");
                if (r != null && !r.isEmpty()) s = r.get(0);
            }}
        }}
        HearthNative.onResult(req, res == RESULT_OK, s);
    }}

    @Override public void onRequestPermissionsResult(int req, String[] p, int[] g) {{
        super.onRequestPermissionsResult(req, p, g);
        for (int i = 0; i < p.length && i < g.length; i++) HearthNative.onPermission(p[i], g[i] == 0);
    }}
}}
"#
        ),
    ));
    if !app.watch {
        v.push((
            "CallService",
            "a foreground Service for calls cannot be defined from Rust (Android instantiates it by class name)",
            format!(
                r#"package {JAVA_PKG};

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.IBinder;

public class CallService extends Service {{
    @Override public IBinder onBind(Intent i) {{ return null; }}

    @Override public int onStartCommand(Intent i, int flags, int id) {{
        if (i != null && "stop".equals(i.getAction())) {{ stopForeground(STOP_FOREGROUND_REMOVE); stopSelf(); return START_NOT_STICKY; }}
        NotificationManager nm = getSystemService(NotificationManager.class);
        nm.createNotificationChannel(new NotificationChannel("call", "Calls", NotificationManager.IMPORTANCE_LOW));
        int icon = getResources().getIdentifier("ic_stat", "drawable", getPackageName());
        String title = i != null && i.hasExtra("title") ? i.getStringExtra("title") : "Hearth";
        String text = i != null && i.hasExtra("text") ? i.getStringExtra("text") : "";
        Notification n = new Notification.Builder(this, "call").setSmallIcon(icon).setContentTitle(title).setContentText(text).setOngoing(true).build();
        startForeground(41, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE);
        return START_NOT_STICKY;
    }}
}}
"#
            ),
        ));
        v.push((
            "WatchLink",
            "WearableListenerService is instantiated by Play services by class name; it only forwards to Rust (HearthNative.watchRequest)",
            format!(
                r#"package {JAVA_PKG};

import com.google.android.gms.tasks.Task;
import com.google.android.gms.tasks.Tasks;
import com.google.android.gms.wearable.WearableListenerService;
import java.util.concurrent.Callable;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class WatchLink extends WearableListenerService {{
    private static final ExecutorService POOL = Executors.newFixedThreadPool(3);

    @Override public Task<byte[]> onRequest(String nodeId, String path, final byte[] request) {{
        if (!"/hearth/rpc".equals(path)) return null;
        final String dir = getFilesDir().getAbsolutePath();
        return Tasks.call(POOL, new Callable<byte[]>() {{ public byte[] call() {{ return HearthNative.watchRequest(dir, request); }} }});
    }}
}}
"#
            ),
        ));
    }
    v
}

// ---------- pipeline ----------

fn gms_jars(t: &Tools, work: &Path) -> R<Vec<PathBuf>> {
    let base = env_path("HEARTH_GMS_DIR", home().join(".gradle/caches/modules-2/files-2.1/com.google.android.gms"));
    let mut jars = vec![];
    for lib in ["play-services-wearable", "play-services-tasks", "play-services-basement", "play-services-base"] {
        let v = newest_subdir(&base.join(lib)).map_err(|e| format!("{e} (the Play services AARs come from the Gradle cache; set HEARTH_GMS_DIR)"))?;
        let aar = fs::read_dir(&v)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .flat_map(|h| fs::read_dir(h.path()).into_iter().flatten().filter_map(|e| e.ok()))
            .map(|e| e.path())
            .find(|p| p.extension().map(|x| x == "aar").unwrap_or(false))
            .ok_or_else(|| format!("no .aar for {lib}"))?;
        let mut z = zip::ZipArchive::new(File::open(&aar).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let out = work.join(format!("{lib}.jar"));
        let mut f = z.by_name("classes.jar").map_err(|e| format!("{}: {e}", aar.display()))?;
        io::copy(&mut f, &mut File::create(&out).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        jars.push(out);
    }
    let _ = t;
    Ok(jars)
}

fn build_apk(app: &App, abis: &[String], debug_lib: bool, skip_rust: bool, out_dir: Option<PathBuf>) -> R<PathBuf> {
    let t = Tools::find()?;
    let version = format!("{}-rust", workspace_version(&t.root));
    let work = t.tmp.join(app.kind);
    let _ = fs::remove_dir_all(work.join("gen"));
    for d in ["gen/res", "gen/java", "classes", "dex"] {
        fs::create_dir_all(work.join(d)).map_err(|e| e.to_string())?;
    }

    // 1. Rust
    let jnilibs = work.join("jniLibs");
    if !skip_rust {
        let mut c = Command::new("cargo");
        c.current_dir(&t.root).env("ANDROID_NDK_HOME", t.ndk()?).env("ANDROID_HOME", &t.sdk).arg("ndk");
        for a in abis {
            c.arg("-t").arg(a);
        }
        c.arg("-P").arg(app.min_sdk.to_string()).arg("-o").arg(&jnilibs).arg("build").arg("-p").arg(app.crate_name).arg("--lib");
        if !debug_lib {
            c.arg("--release");
        }
        run(&mut c)?;
    }

    // 2. generated files
    let gen = work.join("gen");
    fs::write(gen.join("AndroidManifest.xml"), manifest(app, &version)).map_err(|e| e.to_string())?;
    resources(app, &gen.join("res"))?;
    let javas = java_sources(app);
    let jdir = gen.join("java").join(JAVA_PKG.replace('.', "/"));
    fs::create_dir_all(&jdir).map_err(|e| e.to_string())?;
    for (name, _, src) in &javas {
        fs::write(jdir.join(format!("{name}.java")), src).map_err(|e| e.to_string())?;
    }

    // 3. aapt2
    let aapt2 = t.bt.join("aapt2");
    let res_zip = work.join("res.zip");
    run(Command::new(&aapt2).arg("compile").arg("--dir").arg(gen.join("res")).arg("-o").arg(&res_zip))?;
    let base_apk = work.join("base.apk");
    run(Command::new(&aapt2)
        .args(["link", "-I"])
        .arg(&t.android_jar)
        .arg("--manifest")
        .arg(gen.join("AndroidManifest.xml"))
        .args(["--min-sdk-version", &app.min_sdk.to_string(), "--target-sdk-version", "35"])
        .args(["--version-code", &version_code(&version).to_string(), "--version-name", &version])
        .arg("-R")
        .arg(&res_zip)
        .args(["--auto-add-overlay", "-o"])
        .arg(&base_apk))?;

    // 4. javac + d8
    let jars = gms_jars(&t, &work)?;
    let cp = std::iter::once(t.android_jar.clone()).chain(jars.iter().cloned()).map(|p| p.display().to_string()).collect::<Vec<_>>().join(":");
    let classes = work.join("classes");
    let _ = fs::remove_dir_all(&classes);
    fs::create_dir_all(&classes).map_err(|e| e.to_string())?;
    let mut jc = Command::new(&t.javac);
    jc.args(["--release", "8", "-Xlint:-options", "-cp", &cp, "-d"]).arg(&classes);
    for (name, _, _) in &javas {
        jc.arg(jdir.join(format!("{name}.java")));
    }
    run(&mut jc)?;
    let class_files = walk(&classes, "class");
    let dexdir = work.join("dex");
    let _ = fs::remove_dir_all(&dexdir);
    fs::create_dir_all(&dexdir).map_err(|e| e.to_string())?;
    let mut d8 = t.d8();
    d8.args(["--release", "--min-api", &app.min_sdk.to_string(), "--lib"]).arg(&t.android_jar).arg("--output").arg(&dexdir);
    d8.args(&class_files).args(&jars);
    run(&mut d8)?;

    // 5. one apk: aapt2's output + classes*.dex + lib/<abi>/*.so (stored, so they can be mapped straight from the apk)
    let unsigned = work.join("unsigned.apk");
    {
        let mut zin = zip::ZipArchive::new(File::open(&base_apk).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let mut zout = zip::ZipWriter::new(File::create(&unsigned).map_err(|e| e.to_string())?);
        for i in 0..zin.len() {
            let f = zin.by_index_raw(i).map_err(|e| e.to_string())?;
            zout.raw_copy_file(f).map_err(|e| e.to_string())?;
        }
        let deflate = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for dex in walk(&dexdir, "dex") {
            let name = dex.file_name().unwrap().to_string_lossy().to_string();
            zout.start_file(name, deflate).map_err(|e| e.to_string())?;
            zout.write_all(&fs::read(&dex).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
        for a in abis {
            let so = jnilibs.join(a).join(format!("lib{}.so", app.lib));
            if !so.exists() {
                return Err(format!("missing {}", so.display()));
            }
            zout.start_file(format!("lib/{a}/lib{}.so", app.lib), stored).map_err(|e| e.to_string())?;
            zout.write_all(&fs::read(&so).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
        zout.finish().map_err(|e| e.to_string())?;
    }

    // 6. align + sign
    let aligned = work.join("aligned.apk");
    let _ = fs::remove_file(&aligned);
    run(Command::new(t.bt.join("zipalign")).args(["-f", "-P", "16", "4"]).arg(&unsigned).arg(&aligned))?;
    let out_dir = out_dir.unwrap_or_else(|| t.root.join(format!("target/apk")));
    fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let final_apk = out_dir.join(format!("hearth-{}-rust-debug.apk", app.kind));
    let ks = env_path("HEARTH_KEYSTORE", home().join(".android/debug.keystore"));
    run(t.apksigner().arg("sign").arg("--ks").arg(&ks).args(["--ks-pass", "pass:android", "--ks-key-alias", "androiddebugkey", "--key-pass", "pass:android"]).arg("--out").arg(&final_apk).arg(&aligned))?;
    run(t.apksigner().arg("verify").arg("--verbose").arg(&final_apk))?;

    // 7. report
    let badging = run(Command::new(&aapt2).args(["dump", "badging"]).arg(&final_apk))?;
    println!("{}", badging.lines().filter(|l| l.starts_with("package:") || l.starts_with("sdkVersion") || l.starts_with("uses-feature") || l.starts_with("launchable") || l.starts_with("native-code")).collect::<Vec<_>>().join("\n"));
    let size = fs::metadata(&final_apk).map(|m| m.len()).unwrap_or(0);
    println!("\nAPK: {} ({:.1} MB)", final_apk.display(), size as f64 / 1_048_576.0);
    println!("\nJava that remains (generated by hearth-pack, nothing else, no Kotlin):");
    let mut why: BTreeMap<&str, &str> = BTreeMap::new();
    for (n, w, src) in &javas {
        why.insert(n, w);
        println!("  {n}.java ({} lines): {w}", src.lines().count());
    }
    let _ = why;
    Ok(final_apk)
}

fn walk(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut out = vec![];
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p, ext));
            } else if p.extension().map(|x| x == ext).unwrap_or(false) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(kind) = args.next() else {
        eprintln!("usage: hearth-pack <phone|watch> [--abi arm64-v8a,x86_64] [--debug-lib] [--skip-rust] [--out dir]");
        std::process::exit(2);
    };
    let app = match kind.as_str() {
        "phone" => &PHONE,
        "watch" => &WATCH,
        _ => {
            eprintln!("unknown app {kind}");
            std::process::exit(2);
        }
    };
    let (mut abis, mut debug_lib, mut skip, mut out) = (vec!["arm64-v8a".to_string(), "x86_64".to_string()], false, false, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--abi" => abis = args.next().unwrap_or_default().split(',').map(String::from).collect(),
            "--debug-lib" => debug_lib = true,
            "--skip-rust" => skip = true,
            "--out" => out = args.next().map(PathBuf::from),
            _ => {
                eprintln!("unknown option {a}");
                std::process::exit(2);
            }
        }
    }
    if let Err(e) = build_apk(app, &abis, debug_lib, skip, out) {
        eprintln!("hearth-pack: {e}");
        std::process::exit(1);
    }
}
