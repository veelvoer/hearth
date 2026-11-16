//! Foreground service for calls (keeps the process alive while the microphone is open).
//! The service class is generated Java (`CallService`); Android has no way to define one from Rust.

use crate::jvm::with_env;
use jni::objects::JValue;

const CLASS: &str = "dev.voer.hearth.rust.CallService";

fn intent<'a>(env: &mut jni::JNIEnv<'a>, ctx: &jni::objects::JObject, action: &str, title: &str, text: &str) -> jni::errors::Result<jni::objects::JObject<'a>> {
    let pkg = env.call_method(ctx, "getPackageName", "()Ljava/lang/String;", &[])?.l()?;
    let cls = env.new_string(CLASS)?;
    let i = env.new_object("android/content/Intent", "()V", &[])?;
    env.call_method(&i, "setClassName", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;", &[(&pkg).into(), (&cls).into()])?;
    let a = env.new_string(action)?;
    env.call_method(&i, "setAction", "(Ljava/lang/String;)Landroid/content/Intent;", &[(&a).into()])?;
    for (k, v) in [("title", title), ("text", text)] {
        let (k, v) = (env.new_string(k)?, env.new_string(v)?);
        env.call_method(&i, "putExtra", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;", &[(&k).into(), (&v).into()])?;
    }
    Ok(i)
}

/// Start (or update the text of) the call foreground service.
pub fn start_call(title: &str, text: &str) -> crate::Result<()> {
    with_env(|env, ctx| {
        let i = intent(env, ctx, "start", title, text)?;
        env.call_method(ctx, "startForegroundService", "(Landroid/content/Intent;)Landroid/content/ComponentName;", &[(&i).into()])?;
        Ok(())
    })
}

pub fn stop_call() -> crate::Result<()> {
    with_env(|env, ctx| {
        let i = intent(env, ctx, "stop", "", "")?;
        // startService with the stop action lets the service leave the foreground cleanly
        env.call_method(ctx, "startService", "(Landroid/content/Intent;)Landroid/content/ComponentName;", &[(&i).into()])?;
        let _ = JValue::Int(0);
        Ok(())
    })
}
