//! Clipboard (plain text).

use crate::jvm::{system_service, with_env};

pub fn set_text(text: &str) -> crate::Result<()> {
    with_env(|env, ctx| {
        let cm = system_service(env, ctx, "clipboard")?;
        let (l, t) = (env.new_string("Hearth")?, env.new_string(text)?);
        let clip = env
            .call_static_method(
                "android/content/ClipData",
                "newPlainText",
                "(Ljava/lang/CharSequence;Ljava/lang/CharSequence;)Landroid/content/ClipData;",
                &[(&l).into(), (&t).into()],
            )?
            .l()?;
        env.call_method(&cm, "setPrimaryClip", "(Landroid/content/ClipData;)V", &[(&clip).into()])?;
        Ok(())
    })
}

/// The current text on the clipboard (`None` when empty or not text; Android only lets the focused app read it).
pub fn get_text() -> crate::Result<Option<String>> {
    with_env(|env, ctx| {
        let cm = system_service(env, ctx, "clipboard")?;
        let clip = env.call_method(&cm, "getPrimaryClip", "()Landroid/content/ClipData;", &[])?.l()?;
        if clip.is_null() || env.call_method(&clip, "getItemCount", "()I", &[])?.i()? == 0 {
            return Ok(None);
        }
        let item = env.call_method(&clip, "getItemAt", "(I)Landroid/content/ClipData$Item;", &[0.into()])?.l()?;
        let cs = env.call_method(&item, "coerceToText", "(Landroid/content/Context;)Ljava/lang/CharSequence;", &[ctx.into()])?.l()?;
        if cs.is_null() {
            return Ok(None);
        }
        let s = env.call_method(&cs, "toString", "()Ljava/lang/String;", &[])?.l()?;
        Ok(Some(String::from(env.get_string(&s.into())?)))
    })
}
