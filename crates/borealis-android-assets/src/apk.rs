use jni::objects::{JByteArray, JObject, JObjectArray, JString, JValue};
use jni::{JNIEnv, JavaVM};
use std::io;

const CHUNK: usize = 64 * 1024;

fn jerr(e: impl std::fmt::Display) -> io::Error {
    io::Error::other(format!("android assets: {e}"))
}

fn with_assets<T>(
    f: impl FnOnce(&mut JNIEnv, &JObject) -> jni::errors::Result<T>,
) -> io::Result<T> {
    let ctx = ndk_context::android_context();
    // SAFETY: ndk-context holds the live JavaVM pointer for the whole process lifetime.
    let vm = unsafe { JavaVM::from_raw(ctx.vm().cast()) }.map_err(jerr)?;
    let mut env = vm.attach_current_thread().map_err(jerr)?;
    // SAFETY: a global reference to the Android context, owned by ndk-context, never freed here.
    let context = unsafe { JObject::from_raw(ctx.context().cast()) };
    let assets = env
        .call_method(
            &context,
            "getAssets",
            "()Landroid/content/res/AssetManager;",
            &[],
        )
        .and_then(|v| v.l())
        .map_err(jerr)?;
    f(&mut env, &assets).map_err(jerr)
}

/// Names of the entries (files and folders) directly inside the asset folder `dir`.
/// Empty for a file or a missing folder.
///
/// # Errors
/// An I/O error wrapping any JNI failure.
pub fn list(dir: &str) -> io::Result<Vec<String>> {
    with_assets(|env, assets| {
        let path = env.new_string(dir)?;
        let arr = env
            .call_method(
                assets,
                "list",
                "(Ljava/lang/String;)[Ljava/lang/String;",
                &[JValue::Object(&path)],
            )?
            .l()?;
        let arr = JObjectArray::from(arr);
        let len = env.get_array_length(&arr)?;
        let mut names = Vec::new();
        for i in 0..len {
            let s = JString::from(env.get_object_array_element(&arr, i)?);
            names.push(env.get_string(&s)?.into());
        }
        Ok(names)
    })
}

/// The bytes of the asset file at `path`.
///
/// # Errors
/// An I/O error wrapping any JNI failure (including a missing asset).
pub fn read(path: &str) -> io::Result<Vec<u8>> {
    with_assets(|env, assets| {
        let jpath = env.new_string(path)?;
        let stream = env
            .call_method(
                assets,
                "open",
                "(Ljava/lang/String;)Ljava/io/InputStream;",
                &[JValue::Object(&jpath)],
            )?
            .l()?;
        let buf: JByteArray = env.new_byte_array(CHUNK as i32)?;
        let mut chunk = vec![0i8; CHUNK];
        let mut out = Vec::new();
        loop {
            let n = env
                .call_method(&stream, "read", "([B)I", &[JValue::Object(&buf)])?
                .i()?;
            let Ok(n) = usize::try_from(n) else { break };
            env.get_byte_array_region(&buf, 0, &mut chunk[..n])?;
            out.extend(chunk[..n].iter().map(|b| *b as u8));
        }
        env.call_method(&stream, "close", "()V", &[])?;
        Ok(out)
    })
}
