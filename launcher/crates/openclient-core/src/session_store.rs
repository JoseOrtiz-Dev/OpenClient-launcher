//! Session secrets stay out of launcher_state.json and the webview's localStorage.
use crate::identity::MinecraftAccount;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct SavedSession {
    #[serde(default)]
    pub oauth_client_id: String,
    pub account: MinecraftAccount,
    pub refresh_token: String,
    pub expires_at: u64,
}

/// Máximo de cuentas de Microsoft guardadas a la vez.
pub const MAX_ACCOUNTS: usize = 10;

/// `ms-<uuid>`: el id de una cuenta es también el nombre de su archivo, así que se valida.
pub fn valid_account_id(id: &str) -> bool {
    id.strip_prefix("ms-").is_some_and(|rest| {
        (1..=40).contains(&rest.len()) && rest.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
    })
}

/// Carpeta de sesiones: `<root>/sessions`.
pub fn sessions_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join("sessions")
}

/// Archivo protegido de una cuenta: `<root>/sessions/<id>.dpapi`.
pub fn account_file(root: &std::path::Path, id: &str) -> Result<std::path::PathBuf, String> {
    if !valid_account_id(id) {
        return Err("Cuenta de Microsoft inválida.".into());
    }
    Ok(sessions_dir(root).join(format!("{id}.dpapi")))
}

/// Ids de las cuentas con archivo en `sessions/`, ordenados.
pub(crate) fn account_ids(root: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(sessions_dir(root)) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".dpapi").map(str::to_owned))
        .filter(|id| valid_account_id(id))
        .collect();
    ids.sort();
    ids
}

/// Mueve la sesión única de versiones anteriores (`session.dpapi`) a `sessions/<id>.dpapi`.
/// Es idempotente; un archivo ilegible se deja donde está.
pub(crate) fn migrate_legacy(root: &std::path::Path) -> Result<(), String> {
    let legacy = root.join("session.dpapi");
    match std::fs::symlink_metadata(&legacy) {
        Ok(meta) if meta.is_file() => {}
        _ => return Ok(()),
    }
    let Ok(Some(session)) = load(&legacy) else {
        return Ok(());
    };
    let target = account_file(root, &session.account.id)?;
    if !matches!(load(&target), Ok(Some(_))) {
        save(&target, &session)?;
    }
    clear(&legacy)
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(windows)]
fn protect(input: &[u8], decrypt: bool) -> Result<Vec<u8>, String> {
    use std::{ffi::c_void, ptr};
    #[repr(C)]
    struct Blob {
        size: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }
    let size = u32::try_from(input.len()).map_err(|_| "Sesión demasiado grande")?;
    let source = Blob {
        size,
        data: input.as_ptr() as *mut u8,
    };
    let mut output = Blob {
        size: 0,
        data: ptr::null_mut(),
    };
    // No LOCAL_MACHINE flag: protection belongs to the current Windows user.
    let success = unsafe {
        if decrypt {
            CryptUnprotectData(
                &source,
                ptr::null_mut(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut output,
            )
        } else {
            CryptProtectData(
                &source,
                ptr::null(),
                ptr::null(),
                ptr::null_mut(),
                ptr::null_mut(),
                1,
                &mut output,
            )
        }
    };
    if success == 0 {
        return Err(
            "No se pudo acceder a la sesión protegida de Windows. Inicia sesión otra vez.".into(),
        );
    }
    let bytes = unsafe { std::slice::from_raw_parts(output.data, output.size as usize).to_vec() };
    unsafe {
        LocalFree(output.data.cast());
    }
    Ok(bytes)
}

#[cfg(not(windows))]
fn protect(_input: &[u8], _decrypt: bool) -> Result<Vec<u8>, String> {
    Err("El almacenamiento protegido de sesiones requiere Windows.".into())
}

pub(crate) fn load(path: &std::path::Path) -> Result<Option<SavedSession>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("No se pudo leer la sesión protegida.".into()),
    };
    serde_json::from_slice(&protect(&bytes, true)?)
        .map(Some)
        .map_err(|_| "La sesión guardada no es válida. Inicia sesión otra vez.".into())
}
pub(crate) fn save(path: &std::path::Path, session: &SavedSession) -> Result<(), String> {
    let bytes = serde_json::to_vec(session).map_err(|_| "No se pudo preparar la sesión")?;
    let encrypted = protect(&bytes, false)?;
    std::fs::create_dir_all(path.parent().ok_or("Ruta de sesión inválida")?)
        .map_err(|_| "No se pudo crear la carpeta de sesiones")?;
    use std::io::Write;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let write = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp);
    let mut file = write.map_err(|_| "No se pudo preparar la sesión protegida")?;
    let result = file.write_all(&encrypted).and_then(|_| file.sync_all());
    drop(file);
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
        return Err("No se pudo guardar la sesión protegida".into());
    }
    // MoveFileEx performs an atomic replacement, keeping the previous file if it fails.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn MoveFileExW(source: *const u16, target: *const u16, flags: u32) -> i32;
        }
        let from: Vec<u16> = temp.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } == 0 {
            let _ = std::fs::remove_file(&temp);
            return Err("No se pudo reemplazar la sesión protegida".into());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(temp, path).map_err(|_| "No se pudo reemplazar la sesión")?;
    Ok(())
}
pub(crate) fn clear(path: &std::path::Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("No se pudo olvidar la sesión protegida".into()),
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn windows_protection_roundtrip_and_tamper_detection() {
        let data = b"test-only-session-secret";
        let mut encrypted = protect(data, false).unwrap();
        assert!(!encrypted.windows(data.len()).any(|s| s == data));
        assert_eq!(protect(&encrypted, true).unwrap(), data);
        let last = encrypted.len() - 1;
        encrypted[last] ^= 1;
        assert!(protect(&encrypted, true).is_err());
    }
}
