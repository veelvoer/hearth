//! Haptics (needs the VIBRATE permission, which the manifests declare).

use crate::jvm::{system_service, with_env};
use jni::objects::JValue;

#[derive(Clone, Copy, Debug)]
pub enum Effect {
    Tick = 2,
    Click = 0,
    HeavyClick = 5,
}

/// A system haptic effect (tick for small moves, click for presses).
pub fn effect(e: Effect) -> crate::Result<()> {
    with_env(|env, ctx| {
        let v = system_service(env, ctx, "vibrator")?;
        let fx = env.call_static_method("android/os/VibrationEffect", "createPredefined", "(I)Landroid/os/VibrationEffect;", &[JValue::Int(e as i32)])?.l()?;
        env.call_method(&v, "vibrate", "(Landroid/os/VibrationEffect;)V", &[(&fx).into()])?;
        Ok(())
    })
}

/// A plain buzz of `ms` milliseconds.
pub fn buzz(ms: i64) -> crate::Result<()> {
    with_env(|env, ctx| {
        let v = system_service(env, ctx, "vibrator")?;
        let fx = env.call_static_method("android/os/VibrationEffect", "createOneShot", "(JI)Landroid/os/VibrationEffect;", &[JValue::Long(ms), JValue::Int(-1)])?.l()?;
        env.call_method(&v, "vibrate", "(Landroid/os/VibrationEffect;)V", &[(&fx).into()])?;
        Ok(())
    })
}
