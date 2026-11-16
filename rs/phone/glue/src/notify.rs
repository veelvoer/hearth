//! Notifications: channel and post.

use crate::jvm::{icon_id, system_service, with_env};
use jni::objects::JValue;

#[derive(Clone, Copy, Debug)]
pub enum Importance {
    Low = 2,
    Default = 3,
    High = 4,
}

/// Create (or update) a notification channel. Safe to call every start.
pub fn ensure_channel(id: &str, name: &str, importance: Importance) -> crate::Result<()> {
    with_env(|env, ctx| {
        let nm = system_service(env, ctx, "notification")?;
        let (i, n) = (env.new_string(id)?, env.new_string(name)?);
        let ch = env.new_object(
            "android/app/NotificationChannel",
            "(Ljava/lang/String;Ljava/lang/CharSequence;I)V",
            &[(&i).into(), (&n).into(), JValue::Int(importance as i32)],
        )?;
        env.call_method(&nm, "createNotificationChannel", "(Landroid/app/NotificationChannel;)V", &[(&ch).into()])?;
        Ok(())
    })
}

/// Post (or replace, same `tag_id`) a notification; tapping it opens the app.
pub fn post(channel: &str, tag_id: i32, title: &str, text: &str) -> crate::Result<()> {
    with_env(|env, ctx| {
        let nm = system_service(env, ctx, "notification")?;
        let ch = env.new_string(channel)?;
        let b = env.new_object("android/app/Notification$Builder", "(Landroid/content/Context;Ljava/lang/String;)V", &[ctx.into(), (&ch).into()])?;
        let (t, x) = (env.new_string(title)?, env.new_string(text)?);
        env.call_method(&b, "setContentTitle", "(Ljava/lang/CharSequence;)Landroid/app/Notification$Builder;", &[(&t).into()])?;
        env.call_method(&b, "setContentText", "(Ljava/lang/CharSequence;)Landroid/app/Notification$Builder;", &[(&x).into()])?;
        let icon = icon_id(env, ctx)?;
        env.call_method(&b, "setSmallIcon", "(I)Landroid/app/Notification$Builder;", &[JValue::Int(icon)])?;
        env.call_method(&b, "setAutoCancel", "(Z)Landroid/app/Notification$Builder;", &[JValue::Bool(1)])?;
        // tap: launch the app (immutable | update-current)
        let pm = env.call_method(ctx, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[])?.l()?;
        let pkg = env.call_method(ctx, "getPackageName", "()Ljava/lang/String;", &[])?.l()?;
        let launch = env.call_method(&pm, "getLaunchIntentForPackage", "(Ljava/lang/String;)Landroid/content/Intent;", &[(&pkg).into()])?.l()?;
        if !launch.is_null() {
            let pi = env
                .call_static_method(
                    "android/app/PendingIntent",
                    "getActivity",
                    "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;",
                    &[ctx.into(), JValue::Int(0), (&launch).into(), JValue::Int(0x0400_0000 | 0x0800_0000)],
                )?
                .l()?;
            env.call_method(&b, "setContentIntent", "(Landroid/app/PendingIntent;)Landroid/app/Notification$Builder;", &[(&pi).into()])?;
        }
        let n = env.call_method(&b, "build", "()Landroid/app/Notification;", &[])?.l()?;
        env.call_method(&nm, "notify", "(ILandroid/app/Notification;)V", &[JValue::Int(tag_id), (&n).into()])?;
        Ok(())
    })
}

pub fn cancel(tag_id: i32) -> crate::Result<()> {
    with_env(|env, ctx| {
        let nm = system_service(env, ctx, "notification")?;
        env.call_method(&nm, "cancel", "(I)V", &[JValue::Int(tag_id)])?;
        Ok(())
    })
}

/// Ask for the Android 13+ notification permission (the answer arrives through [`crate::results`]).
pub fn request_permission() -> crate::Result<()> {
    crate::intents::request_permission("android.permission.POST_NOTIFICATIONS")
}
