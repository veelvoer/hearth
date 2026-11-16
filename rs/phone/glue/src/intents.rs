//! Open a URL, share, pick a file, take a photo, speech recognition, permissions, reading content URIs.
//! Results come back through callbacks (see [`crate::results`]); they run on an Android thread, so hand
//! the value to the UI with the UI toolkit's own cross-thread call.

use crate::jvm::with_env;
use crate::results::{self, REQ_CAMERA, REQ_PICK, REQ_SPEECH};
use jni::objects::{JObject, JValue};
use jni::JNIEnv;

fn parse_uri<'a>(env: &mut JNIEnv<'a>, s: &str) -> jni::errors::Result<JObject<'a>> {
    let s = env.new_string(s)?;
    env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[(&s).into()])?.l()
}

fn new_intent<'a>(env: &mut JNIEnv<'a>, action: &str) -> jni::errors::Result<JObject<'a>> {
    let a = env.new_string(action)?;
    env.new_object("android/content/Intent", "(Ljava/lang/String;)V", &[(&a).into()])
}

fn put_str(env: &mut JNIEnv, i: &JObject, k: &str, v: &str) -> jni::errors::Result<()> {
    let (k, v) = (env.new_string(k)?, env.new_string(v)?);
    env.call_method(i, "putExtra", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;", &[(&k).into(), (&v).into()])?;
    Ok(())
}

fn set_type(env: &mut JNIEnv, i: &JObject, t: &str) -> jni::errors::Result<()> {
    let t = env.new_string(t)?;
    env.call_method(i, "setType", "(Ljava/lang/String;)Landroid/content/Intent;", &[(&t).into()])?;
    Ok(())
}

/// Open a link in the browser (or whatever handles it).
pub fn open_url(url: &str) -> crate::Result<()> {
    with_env(|env, ctx| {
        let (a, u) = (env.new_string("android.intent.action.VIEW")?, parse_uri(env, url)?);
        let i = env.new_object("android/content/Intent", "(Ljava/lang/String;Landroid/net/Uri;)V", &[(&a).into(), (&u).into()])?;
        env.call_method(ctx, "startActivity", "(Landroid/content/Intent;)V", &[(&i).into()])?;
        Ok(())
    })
}

/// Share plain text through the system share sheet.
pub fn share_text(title: &str, text: &str) -> crate::Result<()> {
    with_env(|env, ctx| {
        let i = new_intent(env, "android.intent.action.SEND")?;
        set_type(env, &i, "text/plain")?;
        put_str(env, &i, "android.intent.extra.TEXT", text)?;
        let t = env.new_string(title)?;
        let ch = env
            .call_static_method("android/content/Intent", "createChooser", "(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;", &[(&i).into(), (&t).into()])?
            .l()?;
        env.call_method(ctx, "startActivity", "(Landroid/content/Intent;)V", &[(&ch).into()])?;
        Ok(())
    })
}

/// Let the user pick a file; `cb` gets the `content://` URI (read it with [`read_uri`]) or `None`.
pub fn pick_file(cb: impl FnOnce(Option<String>) + Send + 'static) -> crate::Result<()> {
    results::expect(REQ_PICK, cb);
    with_env(|env, ctx| {
        let i = new_intent(env, "android.intent.action.OPEN_DOCUMENT")?;
        let cat = env.new_string("android.intent.category.OPENABLE")?;
        env.call_method(&i, "addCategory", "(Ljava/lang/String;)Landroid/content/Intent;", &[(&cat).into()])?;
        set_type(env, &i, "*/*")?;
        env.call_method(ctx, "startActivityForResult", "(Landroid/content/Intent;I)V", &[(&i).into(), JValue::Int(REQ_PICK)])?;
        Ok(())
    })
}

/// Take a photo with the camera app; the picture is stored in the shared Pictures/Hearth folder and `cb`
/// gets its `content://` URI. No camera permission is needed (the camera app does the capture).
pub fn take_photo(name: &str, cb: impl FnOnce(Option<String>) + Send + 'static) -> crate::Result<()> {
    let uri = with_env(|env, ctx| {
        let cv = env.new_object("android/content/ContentValues", "()V", &[])?;
        for (k, v) in [("_display_name", name), ("mime_type", "image/jpeg"), ("relative_path", "Pictures/Hearth")] {
            let (k, v) = (env.new_string(k)?, env.new_string(v)?);
            env.call_method(&cv, "put", "(Ljava/lang/String;Ljava/lang/String;)V", &[(&k).into(), (&v).into()])?;
        }
        let base = env.get_static_field("android/provider/MediaStore$Images$Media", "EXTERNAL_CONTENT_URI", "Landroid/net/Uri;")?.l()?;
        let cr = env.call_method(ctx, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
        let u = env.call_method(&cr, "insert", "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;", &[(&base).into(), (&cv).into()])?.l()?;
        if u.is_null() {
            return Err(jni::errors::Error::NullPtr("MediaStore insert gave no uri"));
        }
        let s = env.call_method(&u, "toString", "()Ljava/lang/String;", &[])?.l()?;
        let uri = String::from(env.get_string(&s.into())?);
        let i = new_intent(env, "android.media.action.IMAGE_CAPTURE")?;
        let ek = env.new_string("output")?;
        env.call_method(&i, "putExtra", "(Ljava/lang/String;Landroid/os/Parcelable;)Landroid/content/Intent;", &[(&ek).into(), (&u).into()])?;
        results::expect(REQ_CAMERA, {
            let uri = uri.clone();
            move |r| cb(r.map(|_| uri))
        });
        env.call_method(ctx, "startActivityForResult", "(Landroid/content/Intent;I)V", &[(&i).into(), JValue::Int(REQ_CAMERA)])?;
        Ok(uri)
    })?;
    let _ = uri;
    Ok(())
}

/// True when something on this device can recognise speech (a watch without a recogniser says false).
pub fn speech_available() -> bool {
    with_env(|env, ctx| {
        let i = new_intent(env, "android.speech.action.RECOGNIZE_SPEECH")?;
        let pm = env.call_method(ctx, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[])?.l()?;
        let r = env.call_method(&pm, "resolveActivity", "(Landroid/content/Intent;I)Landroid/content/pm/ResolveInfo;", &[(&i).into(), JValue::Int(0)])?.l()?;
        Ok(!r.is_null())
    })
    .unwrap_or(false)
}

/// Speech to text through the system recogniser; `cb` gets the best sentence or `None`.
pub fn recognize_speech(prompt: &str, cb: impl FnOnce(Option<String>) + Send + 'static) -> crate::Result<()> {
    results::expect(REQ_SPEECH, cb);
    with_env(|env, ctx| {
        let i = new_intent(env, "android.speech.action.RECOGNIZE_SPEECH")?;
        put_str(env, &i, "android.speech.extra.LANGUAGE_MODEL", "free_form")?;
        put_str(env, &i, "android.speech.extra.PROMPT", prompt)?;
        env.call_method(ctx, "startActivityForResult", "(Landroid/content/Intent;I)V", &[(&i).into(), JValue::Int(REQ_SPEECH)])?;
        Ok(())
    })
}

/// Ask for a runtime permission (answer: [`results::on_permission`]).
pub fn request_permission(perm: &str) -> crate::Result<()> {
    with_env(|env, ctx| {
        let p = env.new_string(perm)?;
        let cls = env.find_class("java/lang/String")?;
        let arr = env.new_object_array(1, cls, &p)?;
        env.call_method(ctx, "requestPermissions", "([Ljava/lang/String;I)V", &[(&arr).into(), JValue::Int(7200)])?;
        Ok(())
    })
}

pub fn has_permission(perm: &str) -> bool {
    with_env(|env, ctx| {
        let p = env.new_string(perm)?;
        Ok(env.call_method(ctx, "checkSelfPermission", "(Ljava/lang/String;)I", &[(&p).into()])?.i()? == 0)
    })
    .unwrap_or(false)
}

/// Read a whole `content://` (or file) URI, for example a picked file. Mind the size before calling.
pub fn read_uri(uri: &str) -> crate::Result<Vec<u8>> {
    with_env(|env, ctx| {
        let u = parse_uri(env, uri)?;
        let cr = env.call_method(ctx, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
        let st = env.call_method(&cr, "openInputStream", "(Landroid/net/Uri;)Ljava/io/InputStream;", &[(&u).into()])?.l()?;
        if st.is_null() {
            return Err(jni::errors::Error::NullPtr("no stream"));
        }
        let buf = env.new_byte_array(64 * 1024)?;
        let mut out = Vec::new();
        let mut tmp = vec![0i8; 64 * 1024];
        loop {
            let n = env.call_method(&st, "read", "([B)I", &[(&buf).into()])?.i()?;
            if n < 0 {
                break;
            }
            env.get_byte_array_region(&buf, 0, &mut tmp[..n as usize])?;
            out.extend(tmp[..n as usize].iter().map(|b| *b as u8));
        }
        env.call_method(&st, "close", "()V", &[])?;
        Ok(out)
    })
}

/// The display name of a content URI (the file name the user sees), if the provider knows it.
pub fn display_name(uri: &str) -> crate::Result<Option<String>> {
    with_env(|env, ctx| {
        let u = parse_uri(env, uri)?;
        let cr = env.call_method(ctx, "getContentResolver", "()Landroid/content/ContentResolver;", &[])?.l()?;
        let null = JObject::null();
        let c = env
            .call_method(
                &cr,
                "query",
                "(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",
                &[(&u).into(), (&null).into(), (&null).into(), (&null).into(), (&null).into()],
            )?
            .l()?;
        if c.is_null() {
            return Ok(None);
        }
        let mut name = None;
        if env.call_method(&c, "moveToFirst", "()Z", &[])?.z()? {
            let col = env.new_string("_display_name")?;
            let ix = env.call_method(&c, "getColumnIndex", "(Ljava/lang/String;)I", &[(&col).into()])?.i()?;
            if ix >= 0 {
                let s = env.call_method(&c, "getString", "(I)Ljava/lang/String;", &[JValue::Int(ix)])?.l()?;
                if !s.is_null() {
                    name = Some(String::from(env.get_string(&s.into())?));
                }
            }
        }
        env.call_method(&c, "close", "()V", &[])?;
        Ok(name)
    })
}
