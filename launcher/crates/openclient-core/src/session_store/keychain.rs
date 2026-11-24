// El archivo local solo referencia una entrada en el llavero del usuario.
const PREFIX: &str = "OpenClient-Keychain:v1:";
#[cfg(target_os = "macos")]
const SERVICE: &str = "com.openclient.launcher.microsoft";

pub(super) fn locator(bytes: &[u8]) -> Option<&str> {
    let id = std::str::from_utf8(bytes).ok()?.strip_prefix(PREFIX)?;
    let uuid = uuid::Uuid::parse_str(id).ok()?;
    (uuid.to_string() == id).then_some(id)
}

pub(super) fn marker(id: &str) -> Vec<u8> {
    format!("{PREFIX}{id}").into_bytes()
}

#[cfg(target_os = "macos")]
pub(super) fn read(id: &str) -> Result<Vec<u8>, String> {
    use security_framework::passwords::{generic_password, PasswordOptions};

    generic_password(PasswordOptions::new_generic_password(SERVICE, id))
        .map_err(|_| "No se encontró la sesión en el llavero de macOS. Inicia sesión otra vez.".into())
}

#[cfg(target_os = "macos")]
pub(super) fn write(id: &str, bytes: &[u8]) -> Result<(), String> {
    security_framework::passwords::set_generic_password(SERVICE, id, bytes)
        .map_err(|_| "Desbloquea el llavero de macOS para guardar la sesión de Microsoft.".into())
}

#[cfg(target_os = "macos")]
pub(super) fn remove(id: &str) -> Result<(), String> {
    match security_framework::passwords::delete_generic_password(SERVICE, id) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == -25300 => Ok(()), // errSecItemNotFound
        Err(_) => Err("No se pudo borrar la sesión del llavero de macOS.".into()),
    }
}

#[cfg(target_os = "linux")]
mod secret_service {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    const ERROR: &str = "Abre o desbloquea el llavero de tu escritorio e instala secret-tool para usar cuentas de Microsoft.";

    fn command(action: &str, id: &str) -> Command {
        let mut command = Command::new("secret-tool");
        command.arg(action);
        if action == "store" {
            command.arg("--label=OpenClient · Microsoft");
        }
        command.args(["application", "openclient", "session", id]);
        command
    }

    pub(crate) fn read(id: &str) -> Result<Vec<u8>, String> {
        let output = command("lookup", id).output().map_err(|_| ERROR)?;
        if !output.status.success() || output.stdout.is_empty() {
            return Err(ERROR.into());
        }
        Ok(output.stdout)
    }

    pub(crate) fn write(id: &str, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() >= 8192 {
            return Err("La sesión supera el tamaño que admite secret-tool.".into());
        }
        let mut child = command("store", id)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| ERROR)?;
        let written = child.stdin.take().ok_or(ERROR)?.write_all(bytes);
        let status = child.wait().map_err(|_| ERROR)?;
        if written.is_err() || !status.success() {
            return Err(ERROR.into());
        }
        Ok(())
    }

    pub(crate) fn remove(id: &str) -> Result<(), String> {
        let output = command("clear", id).output().map_err(|_| ERROR)?;
        // secret-tool devuelve 1 sin diagnóstico si el elemento ya no está.
        if output.status.success() || (output.status.code() == Some(1) && output.stderr.is_empty()) {
            return Ok(());
        }
        Err(ERROR.into())
    }
}

#[cfg(target_os = "linux")]
pub(super) use secret_service::{read, remove, write};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_marker_contains_only_a_canonical_identifier() {
        let id = "11111111-1111-4111-8111-111111111111";
        assert_eq!(locator(&marker(id)), Some(id));
        assert_eq!(locator(&marker("../../launcher_state.json")), None);
        assert_eq!(locator(b"existing DPAPI data"), None);
    }
}
