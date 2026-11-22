fn usable_state(content: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .is_some_and(|state| state["instances"].is_array())
}

pub fn load_state_at(path: &std::path::Path) -> Result<Option<String>, String> {
    let backup = path.with_extension("json.bak");
    for candidate in [path, backup.as_path()] {
        if let Ok(content) = std::fs::read_to_string(candidate) {
            if usable_state(&content) {
                return Ok(Some(content));
            }
        }
    }
    if !path.exists() && !backup.exists() {
        return Ok(None);
    }
    Err("No se pudo leer la biblioteca ni su copia .bak; conserva los archivos antes de recuperarla.".into())
}

pub fn write_state_locked(path: &std::path::Path, state_json: &str) -> Result<(), String> {
    if !usable_state(state_json) {
        return Err("Estado de biblioteca inválido".into());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Error creando la carpeta de datos de OpenClient: {}", e))?;
    }
    if path.exists() {
        let previous = std::fs::read_to_string(path)
            .map_err(|e| format!("No se pudo respaldar la biblioteca: {}", e))?;
        let backup = if usable_state(&previous) {
            path.with_extension("json.bak")
        } else {
            path.with_extension(format!("corrupt-{}.json", uuid::Uuid::new_v4()))
        };
        std::fs::write(&backup, previous)
            .map_err(|e| format!("No se pudo conservar el estado anterior: {}", e))?;
    }
    std::fs::write(path.with_extension("json.tmp"), state_json)
        .and_then(|_| std::fs::rename(path.with_extension("json.tmp"), path))
        .map_err(|e| format!("Error guardando estado de OpenClient: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod state_tests {
    use super::*;
    #[test]
    fn state_save_preserves_valid_backup_and_archives_corrupt_input() {
        let root =
            std::env::temp_dir().join(format!("openclient-state-test-{}", uuid::Uuid::new_v4()));
        let path = root.join("launcher_state.json");
        write_state_locked(&path, r#"{"instances":[]}"#).unwrap();
        write_state_locked(&path, r#"{"instances":[{"id":"demo"}]}"#).unwrap();
        assert_eq!(
            std::fs::read_to_string(path.with_extension("json.bak")).unwrap(),
            r#"{"instances":[]}"#
        );
        assert!(write_state_locked(&path, "bad-json").is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"instances":[{"id":"demo"}]}"#
        );
        std::fs::write(&path, "broken-original").unwrap();
        assert_eq!(
            load_state_at(&path).unwrap().unwrap(),
            r#"{"instances":[]}"#
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken-original");
        write_state_locked(&path, r#"{"instances":[]}"#).unwrap();
        let corrupt = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| e.file_name().to_string_lossy().contains("corrupt-"))
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(corrupt.path()).unwrap(),
            "broken-original"
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
