//! Wear OS data layer, the same protocol as `WatchLink.kt` (phone) and `Api.kt` (watch):
//! the watch sends a MessageClient request to `/hearth/rpc` with JSON `{op, base, id, text}` and gets back
//! one JSON object: `{usage}`, `{chats}`, `{asked, answer}`, `{code}` or `{error[, code]}`.
//!
//! Watch side: [`ask`] (needs the `play-services-wearable` classes in the APK; hearth-pack bundles them).
//! Phone side: the generated Java `WatchLink` calls the native `watchRequest`, which ends in the handler given to
//! [`crate::export_natives!`]; [`serve`] is the JSON in/out shell for it. The phone declares the capability
//! [`CAPABILITY`] in `res/values/wear.xml` so watches can find it.

use crate::jvm::{load_class, with_env};
use jni::objects::{JObject, JValue};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const PATH: &str = "/hearth/rpc";
pub const CAPABILITY: &str = "hearth_phone_link";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub op: String,
    #[serde(default)]
    pub base: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ChatRow {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub busy: bool,
    #[serde(default)]
    pub mtime: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Reply {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chats: Option<Vec<ChatRow>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Reply {
    pub fn error(msg: impl Into<String>) -> Self {
        Reply { error: Some(msg.into()), ..Default::default() }
    }
    pub fn code(code: i64) -> Self {
        Reply { code: Some(code), ..Default::default() }
    }
    /// The user-facing message of an error reply, like the Kotlin watch shows it.
    pub fn error_text(&self, server: &str) -> Option<String> {
        let e = self.error.as_ref()?;
        Some(match (self.code, e.as_str()) {
            (Some(401), _) => "The phone needs to connect again".into(),
            (Some(_), e) => format!("{server} {e}"),
            (None, e) => e.into(),
        })
    }
}

/// Phone side: bytes in, bytes out. Wraps `answer` so a bad request never throws into Java.
pub fn serve(request: &[u8], answer: impl FnOnce(Request) -> Reply) -> Vec<u8> {
    let reply = match serde_json::from_slice::<Request>(request) {
        Ok(q) => answer(q),
        Err(_) => Reply::error("bad request"),
    };
    serde_json::to_vec(&reply).unwrap_or_else(|_| br#"{"error":"failed"}"#.to_vec())
}

/// Ids a phone accepts (same rule as the Kotlin phone side).
pub fn valid_id(id: &str) -> bool {
    id.is_empty() || (id.len() <= 80 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}

// ---- watch side (JNI into play-services-wearable) ----

struct NodeCache {
    node: Option<String>,
    at: Option<Instant>,
}
static NODE: Mutex<NodeCache> = Mutex::new(NodeCache { node: None, at: None });

fn await_task<'a>(env: &mut jni::JNIEnv<'a>, ctx: &JObject, task: &JObject, secs: i64) -> jni::errors::Result<JObject<'a>> {
    let tasks = load_class(env, ctx, "com.google.android.gms.tasks.Tasks")?;
    let unit = env.get_static_field("java/util/concurrent/TimeUnit", "SECONDS", "Ljava/util/concurrent/TimeUnit;")?.l()?;
    env.call_static_method(
        &tasks,
        "await",
        "(Lcom/google/android/gms/tasks/Task;JLjava/util/concurrent/TimeUnit;)Ljava/lang/Object;",
        &[task.into(), JValue::Long(secs), (&unit).into()],
    )?
    .l()
}

/// The id of a reachable phone that has Hearth (nearby first). Blocks up to 4 s: never call on the UI thread.
pub fn find_phone() -> crate::Result<Option<String>> {
    with_env(|env, ctx| {
        let w = load_class(env, ctx, "com.google.android.gms.wearable.Wearable")?;
        let cc = env
            .call_static_method(&w, "getCapabilityClient", "(Landroid/content/Context;)Lcom/google/android/gms/wearable/CapabilityClient;", &[ctx.into()])?
            .l()?;
        let cap = env.new_string(CAPABILITY)?;
        // FILTER_REACHABLE = 1
        let task = env
            .call_method(&cc, "getCapability", "(Ljava/lang/String;I)Lcom/google/android/gms/tasks/Task;", &[(&cap).into(), JValue::Int(1)])?
            .l()?;
        let info = await_task(env, ctx, &task, 4)?;
        let nodes = env.call_method(&info, "getNodes", "()Ljava/util/Set;", &[])?.l()?;
        let it = env.call_method(&nodes, "iterator", "()Ljava/util/Iterator;", &[])?.l()?;
        let mut first: Option<String> = None;
        while env.call_method(&it, "hasNext", "()Z", &[])?.z()? {
            let n = env.call_method(&it, "next", "()Ljava/lang/Object;", &[])?.l()?;
            let id = env.call_method(&n, "getId", "()Ljava/lang/String;", &[])?.l()?;
            let id = String::from(env.get_string(&id.into())?);
            if env.call_method(&n, "isNearby", "()Z", &[])?.z()? {
                return Ok(Some(id));
            }
            first.get_or_insert(id);
        }
        Ok(first)
    })
}

/// One request to a given node (blocks up to 30 s).
pub fn request(node: &str, q: &Request) -> crate::Result<Reply> {
    let body = serde_json::to_vec(q).map_err(|e| crate::Error::Other(e.to_string()))?;
    let bytes = with_env(|env, ctx| {
        let w = load_class(env, ctx, "com.google.android.gms.wearable.Wearable")?;
        let mc = env
            .call_static_method(&w, "getMessageClient", "(Landroid/content/Context;)Lcom/google/android/gms/wearable/MessageClient;", &[ctx.into()])?
            .l()?;
        let (n, p) = (env.new_string(node)?, env.new_string(PATH)?);
        let arr = env.byte_array_from_slice(&body)?;
        let task = env
            .call_method(
                &mc,
                "sendRequest",
                "(Ljava/lang/String;Ljava/lang/String;[B)Lcom/google/android/gms/tasks/Task;",
                &[(&n).into(), (&p).into(), (&arr).into()],
            )?
            .l()?;
        let res = await_task(env, ctx, &task, 30)?;
        env.convert_byte_array(jni::objects::JByteArray::from(res))
    })?;
    serde_json::from_slice(&bytes).map_err(|e| crate::Error::Other(e.to_string()))
}

/// Ask the phone. `Ok(None)` = no phone in reach (the caller then talks to the server itself, like the Kotlin watch);
/// the phone node is looked up at most every 30 s and forgotten when a request fails.
pub fn ask(q: &Request) -> crate::Result<Option<Reply>> {
    let cached = {
        let c = NODE.lock().map_err(|_| crate::Error::Other("lock".into()))?;
        match (&c.node, c.at) {
            (n, Some(t)) if t.elapsed() < Duration::from_secs(30) => Some(n.clone()),
            _ => None,
        }
    };
    let node = match cached {
        Some(n) => n,
        None => {
            let n = find_phone().unwrap_or(None);
            let mut c = NODE.lock().map_err(|_| crate::Error::Other("lock".into()))?;
            c.node = n.clone();
            c.at = Some(Instant::now());
            n
        }
    };
    let Some(node) = node else { return Ok(None) };
    match request(&node, q) {
        Ok(r) => Ok(Some(r)),
        Err(_) => {
            if let Ok(mut c) = NODE.lock() {
                c.at = None;
            }
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_and_shapes() {
        let out = serve(br#"{"op":"chats","base":"http://x"}"#, |q| {
            assert_eq!(q.op, "chats");
            Reply { chats: Some(vec![ChatRow { id: "a".into(), title: "T".into(), busy: true, mtime: 5 }]), ..Default::default() }
        });
        assert_eq!(std::str::from_utf8(&out).unwrap(), r#"{"chats":[{"id":"a","title":"T","busy":true,"mtime":5}]}"#);
        assert!(std::str::from_utf8(&serve(b"nope", |_| Reply::default())).unwrap().contains("bad request"));
        assert!(valid_id("abc-1_2") && !valid_id("a/b") && valid_id(""));
        assert_eq!(Reply { code: Some(401), error: Some("x".into()), ..Default::default() }.error_text("S").unwrap(), "The phone needs to connect again");
    }
}
