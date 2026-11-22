//! Acción `crash_analyze`: diagnóstico de solo lectura del último registro de una instancia.

use openclient_core::crash_analysis::{analyze, Context};
use serde_json::{json, Value};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

const TAIL_BYTES: u64 = 256 * 1024;
const HEAD_BYTES: u64 = 4096;

/// Lee el principio (cabecera `[OpenClient]`, solo para el contexto) y el final del log; vacío si no existe.
fn read_log(root: &Path, id: &str) -> (String, String) {
    let dir = root.join("instances").join(id).join("logs");
    let path = ["openclient-latest.log", "alba-launcher-latest.log"]
        .iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file());
    let Some(mut file) = path.and_then(|path| File::open(path).ok()) else {
        return Default::default();
    };
    let len = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let mut read = |start: u64, max: u64| {
        let mut bytes = Vec::new();
        let _ = file.seek(SeekFrom::Start(start));
        let _ = (&mut file).take(max).read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    };
    let head = read(0, HEAD_BYTES.min(len));
    let start = len.saturating_sub(TAIL_BYTES);
    let mut tail = read(start, TAIL_BYTES);
    if start > 0 {
        // La primera línea puede estar cortada a mitad.
        tail = tail.split_once('\n').map(|(_, rest)| rest.to_owned()).unwrap_or_default();
    }
    (head, tail)
}

/// `[OpenClient] Minecraft X (loader) | Java N | ...` -> N.
fn header_java(head: &str) -> Option<u32> {
    let line = head.lines().find(|line| line.starts_with("[OpenClient] Minecraft "))?;
    let rest = &line[line.find("| Java ")? + "| Java ".len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|major| *major > 0)
}

/// `[OpenClient] RAM: N MB | ...` -> N.
fn header_ram(head: &str) -> Option<u32> {
    let line = head.lines().find(|line| line.starts_with("[OpenClient] RAM: "))?;
    let digits: String = line["[OpenClient] RAM: ".len()..].chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok().filter(|ram| *ram > 0)
}

pub fn analyze_crash(request: &Value) -> Result<Value, String> {
    let (root, instance) = crate::game::selection(request)?;
    let id = instance["id"].as_str().ok_or("Identificador de instancia inválido.")?;
    let (head, tail) = read_log(root, id);
    let from_u32 = |value: &Value| value.as_u64().filter(|v| *v > 0).and_then(|v| u32::try_from(v).ok());
    let context = Context {
        mc_version: instance["mcVersion"].as_str().unwrap_or_default().to_owned(),
        loader: instance["category"].as_str().unwrap_or_default().to_owned(),
        java_major: header_java(&head),
        ram_mb: header_ram(&head)
            .or_else(|| from_u32(&instance["ramMb"]))
            .or_else(|| from_u32(&request["ram"])),
        system_ram_mb: Some(openclient_core::performance::detect_system_hardware().total_ram_mb).filter(|ram| *ram > 0),
    };
    Ok(json!({ "findings": analyze(&tail, &context) }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(log: Option<&str>) -> (std::path::PathBuf, String) {
        let root = std::env::temp_dir().join(format!("openclient-crash-{}", uuid::Uuid::new_v4()));
        let id = uuid::Uuid::new_v4().to_string();
        std::fs::create_dir_all(root.join("instances").join(&id).join("logs")).unwrap();
        let state = json!({"instances": [{"id": id, "name": "Prueba", "mcVersion": "1.21.4", "category": "fabric", "ramMb": 4096}]});
        std::fs::write(root.join("launcher_state.json"), state.to_string()).unwrap();
        if let Some(log) = log {
            std::fs::write(root.join("instances").join(&id).join("logs/openclient-latest.log"), log).unwrap();
        }
        (root, id)
    }

    #[test]
    fn analyzes_log_with_header_context() {
        let log = "[OpenClient] Minecraft 1.21.4 (fabric) | Java 17 | C:/java/bin/java.exe\n[OpenClient] Usuario: Test (offline)\n[OpenClient] RAM: 2048 MB | Clase principal: x\n\nException: java.lang.OutOfMemoryError: Java heap space\n";
        let (root, id) = library(Some(log));
        let result = analyze_crash(&json!({"action": "crash_analyze", "root": root, "id": id})).unwrap();
        let findings = result["findings"].as_array().unwrap();
        assert_eq!(findings[0]["id"], "out_of_memory");
        assert_eq!(findings[0]["severity"], "error");
        assert!(findings[0]["detail"].as_str().unwrap().contains("2048 MB"));
        for key in ["title", "action"] {
            assert!(findings[0][key].as_str().is_some_and(|text| !text.is_empty()));
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn huge_log_keeps_header_and_only_analyzes_the_tail() {
        let mut log = String::from("[OpenClient] Minecraft 1.21.4 (fabric) | Java 8 | j\n[OpenClient] RAM: 1024 MB | Clase principal: x\n");
        log.push_str("java.lang.OutOfMemoryError: viejo\n");
        log.push_str(&"linea de relleno sin problemas\n".repeat(12_000));
        log.push_str("java.lang.UnsupportedClassVersionError: x (class file version 65.0)\n");
        let (root, id) = library(Some(&log));
        let result = analyze_crash(&json!({"root": root, "id": id})).unwrap();
        let findings = result["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0]["id"], "java_version");
        assert!(findings[0]["detail"].as_str().unwrap().contains("Java 8"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_log_or_bad_id_is_handled() {
        let (root, id) = library(None);
        let result = analyze_crash(&json!({"root": root, "id": id})).unwrap();
        assert_eq!(result["findings"], json!([]));
        assert!(analyze_crash(&json!({"root": root, "id": "../x"})).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
