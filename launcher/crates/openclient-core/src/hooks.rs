//! Comandos previo, posterior y envoltorio del lanzamiento.
//!
//! Se ejecutan sin shell: el texto se divide en programa y argumentos, así que
//! `&`, `|` o `%VAR%` no tienen significado especial.

use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const COMMAND_LIMIT: usize = 512;
const TIMEOUT: Duration = Duration::from_secs(60);

pub fn split_command(text: &str) -> Result<Vec<String>, String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut started = false;
    for character in text.chars() {
        match character {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            c if c.is_whitespace() && !quoted => {
                if started {
                    parts.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            c => {
                current.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err("Falta cerrar unas comillas.".into());
    }
    if started {
        parts.push(current);
    }
    if parts.first().is_none_or(|program| program.is_empty()) {
        return Err("Escribe un programa que ejecutar.".into());
    }
    Ok(parts)
}

/// Devuelve el comando recortado. Vacío significa «sin comando».
pub fn validate(text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(String::new());
    }
    if text.chars().count() > COMMAND_LIMIT || text.chars().any(char::is_control) {
        return Err(format!(
            "El comando debe tener como máximo {COMMAND_LIMIT} caracteres y ninguno de control."
        ));
    }
    split_command(text)?;
    Ok(text.to_owned())
}

pub fn run(command: &str, directory: &Path, environment: &[(&str, String)]) -> Result<(), String> {
    let parts = split_command(command)?;
    let mut process = Command::new(&parts[0]);
    process
        .args(&parts[1..])
        .current_dir(directory)
        .envs(environment.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        process.creation_flags(0x08000000);
    }
    let mut child = process
        .spawn()
        .map_err(|error| format!("No se pudo ejecutar «{}»: {error}", parts[0]))?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => {
                return Err(match status.code() {
                    Some(code) => format!("terminó con código {code}."),
                    None => "terminó de forma anómala.".into(),
                })
            }
            None if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("tardó más de {} segundos.", TIMEOUT.as_secs()));
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quotes_without_shell_meaning() {
        assert_eq!(
            split_command(r#""C:\Program Files\Tool\tool.exe" --a "b c" d&e"#).unwrap(),
            vec![r"C:\Program Files\Tool\tool.exe", "--a", "b c", "d&e"]
        );
        assert!(split_command(r#"tool "sin cerrar"#).is_err());
        assert!(split_command("   ").is_err());
        assert!(split_command(r#""""#).is_err());
    }

    #[test]
    fn validation_trims_and_rejects_control_characters_and_length() {
        assert_eq!(validate("  tool.exe -x  ").unwrap(), "tool.exe -x");
        assert_eq!(validate("   ").unwrap(), "");
        assert!(validate("tool\u{7}.exe").is_err());
        assert!(validate("tool\nother").is_err());
        assert!(validate(&"a".repeat(COMMAND_LIMIT + 1)).is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn reports_exit_codes_and_missing_programs() {
        let directory = std::env::temp_dir();
        assert!(run("cmd /c exit 0", &directory, &[]).is_ok());
        assert!(run("cmd /c exit 3", &directory, &[])
            .unwrap_err()
            .contains("código 3"));
        assert!(run("openclient-no-existe-12345", &directory, &[]).is_err());
        let value = format!("v{}", uuid::Uuid::new_v4().simple());
        assert!(run(
            &format!("cmd /c if \"%OPENCLIENT_TEST%\"==\"{value}\" (exit 0) else (exit 9)"),
            &directory,
            &[("OPENCLIENT_TEST", value.clone())]
        )
        .is_ok());
    }
}
