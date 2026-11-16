//! The native methods that the generated Java (`HearthNative`) declares. Rust cannot register them
//! from an rlib reliably, so each app crate invokes [`export_natives!`] once, which defines the
//! `Java_dev_voer_hearth_rust_HearthNative_*` symbols inside the cdylib itself.
//!
//! ```ignore
//! fn watch(files_dir: &str, request: &[u8]) -> Vec<u8> { /* answer a watch RPC */ }
//! hearth_android::export_natives!(watch);
//! ```

/// `$watch` is `fn(files_dir: &str, request: &[u8]) -> Vec<u8>`.
#[macro_export]
macro_rules! export_natives {
    ($watch:path) => {
        #[no_mangle]
        pub extern "system" fn Java_dev_voer_hearth_rust_HearthNative_onResult(
            mut env: $crate::natives::jni::JNIEnv,
            _class: $crate::natives::jni::objects::JClass,
            req: $crate::natives::jni::sys::jint,
            ok: $crate::natives::jni::sys::jboolean,
            data: $crate::natives::jni::objects::JString,
        ) {
            let data = if data.is_null() { None } else { env.get_string(&data).ok().map(String::from) };
            $crate::results::deliver(req, ok != 0, data);
        }

        #[no_mangle]
        pub extern "system" fn Java_dev_voer_hearth_rust_HearthNative_onPermission(
            mut env: $crate::natives::jni::JNIEnv,
            _class: $crate::natives::jni::objects::JClass,
            perm: $crate::natives::jni::objects::JString,
            granted: $crate::natives::jni::sys::jboolean,
        ) {
            if let Ok(p) = env.get_string(&perm) {
                $crate::results::deliver_permission(&String::from(p), granted != 0);
            }
        }

        #[no_mangle]
        pub extern "system" fn Java_dev_voer_hearth_rust_HearthNative_watchRequest<'l>(
            env: $crate::natives::jni::JNIEnv<'l>,
            _class: $crate::natives::jni::objects::JClass<'l>,
            files_dir: $crate::natives::jni::objects::JString<'l>,
            request: $crate::natives::jni::objects::JByteArray<'l>,
        ) -> $crate::natives::jni::sys::jbyteArray {
            $crate::natives::watch_request(env, files_dir, request, $watch)
        }
    };
}

pub use jni;

use jni::objects::{JByteArray, JString};
use jni::JNIEnv;

#[doc(hidden)]
pub fn watch_request<'l>(
    mut env: JNIEnv<'l>,
    files_dir: JString<'l>,
    request: JByteArray<'l>,
    handler: fn(&str, &[u8]) -> Vec<u8>,
) -> jni::sys::jbyteArray {
    let dir = env.get_string(&files_dir).map(String::from).unwrap_or_default();
    let req = env.convert_byte_array(&request).unwrap_or_default();
    let out = handler(&dir, &req);
    env.byte_array_from_slice(&out).map(|a| a.into_raw()).unwrap_or(std::ptr::null_mut())
}
