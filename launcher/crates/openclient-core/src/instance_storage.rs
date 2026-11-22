use serde_json::Value;
use std::path::Path;

pub fn duplicate_at(
    root: &Path,
    source_id: &str,
    target_id: &str,
    state_json: &str,
) -> Result<(), String> {
    let valid = |id: &str| uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id);
    if !valid(source_id) || !valid(target_id) || source_id == target_id {
        return Err("Identificador de instancia inválido.".into());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let parent = root.join("instances");
    let source = parent.join(source_id);
    let target = parent.join(target_id);
    let state = root.join("launcher_state.json");
    if !parent.exists() {
        return crate::library_state::write_state_locked(&state, state_json);
    }
    if parent.canonicalize().map_err(|error| error.to_string())? != parent {
        return Err("La carpeta de instancias apunta a otra ubicación.".into());
    }
    match std::fs::symlink_metadata(&source) {
        Ok(metadata) => {
            if !metadata.is_dir()
                || source.canonicalize().map_err(|error| error.to_string())? != source
            {
                return Err("La carpeta de origen apunta a otra ubicación.".into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return crate::library_state::write_state_locked(&state, state_json);
        }
        Err(error) => return Err(format!("No se pudo leer la instancia: {error}")),
    }
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err("La carpeta de destino ya existe.".into());
    }
    publish_instance(&root, &target, state_json, |staging| {
        staging.copy_from(&source)
    })
}

pub(crate) fn publish_instance(
    root: &Path,
    target: &Path,
    state_json: &str,
    prepare: impl FnOnce(&crate::directory_install::PreparedDirectory) -> Result<(), String>,
) -> Result<(), String> {
    if std::fs::symlink_metadata(target).is_ok() {
        return Err("La carpeta de destino ya existe.".into());
    }
    let staging = crate::directory_install::PreparedDirectory::new(target, false)?;
    prepare(&staging)?;
    // A plain rename never replaces an existing destination directory on Windows.
    std::fs::rename(&staging.path, target)
        .map_err(|error| format!("No se pudo guardar la copia: {error}"))?;
    if let Err(error) =
        crate::library_state::write_state_locked(&root.join("launcher_state.json"), state_json)
    {
        if let Err(restore) = std::fs::rename(target, &staging.path) {
            return Err(format!(
                "{error}. La copia quedó en {} ({restore}).",
                target.display()
            ));
        }
        return Err(error);
    }
    Ok(())
}

/// Identificador de carpeta de instancia seguro como componente de ruta.
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 180
        && !id.starts_with('.')
        && !id.ends_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

pub fn delete_at(root: &Path, instance_id: &str, state_json: &str) -> Result<String, String> {
    delete_with_writer(
        root,
        instance_id,
        state_json,
        crate::library_state::write_state_locked,
    )
}

fn delete_with_writer(
    root: &Path,
    instance_id: &str,
    state_json: &str,
    write: impl FnOnce(&Path, &str) -> Result<(), String>,
) -> Result<String, String> {
    if !valid_id(instance_id) {
        return Err("Identificador de instancia inválido.".into());
    }
    let next: Value = serde_json::from_str(state_json).map_err(|_| "Estado inválido")?;
    let remaining = next["instances"]
        .as_array()
        .ok_or("Falta la lista de instancias")?;
    if remaining
        .iter()
        .any(|i| i["id"].as_str() == Some(instance_id))
    {
        return Err("La instancia todavía está incluida en el estado nuevo.".into());
    }
    let state_path = root.join("launcher_state.json");
    let old: Value = serde_json::from_str(
        &crate::library_state::load_state_at(&state_path)?.ok_or("La biblioteca aún no existe")?,
    )
    .map_err(|_| "Biblioteca inválida")?;
    let deleted = old["instances"]
        .as_array()
        .and_then(|items| items.iter().find(|i| i["id"].as_str() == Some(instance_id)))
        .ok_or("La instancia ya no existe en la biblioteca guardada")?;

    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let source = root.join("instances").join(instance_id);
    if source.exists() {
        let resolved = source.canonicalize().map_err(|e| e.to_string())?;
        if !resolved.starts_with(root.join("instances")) {
            return Err("La carpeta de la instancia apunta fuera de la biblioteca.".into());
        }
        let metadata = std::fs::symlink_metadata(&source).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("No se pueden eliminar enlaces de carpetas.".into());
            }
        }
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("La ruta de la instancia no es una carpeta normal.".into());
        }
    }
    let trash = root.join("trash");
    std::fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    if !trash
        .canonicalize()
        .map_err(|e| e.to_string())?
        .starts_with(&root)
    {
        return Err("La carpeta de recuperación apunta fuera del launcher.".into());
    }
    // Las copias de seguridad del modpack viajan con la instancia a la papelera.
    let snapshots = root.join("snapshots").join(instance_id);
    let has_snapshots = match std::fs::symlink_metadata(&snapshots) {
        Ok(metadata) => {
            #[cfg(windows)]
            let linked = {
                use std::os::windows::fs::MetadataExt;
                metadata.file_attributes() & 0x400 != 0
            };
            #[cfg(not(windows))]
            let linked = false;
            if linked || metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("La carpeta de copias de seguridad no es una carpeta normal.".into());
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.to_string()),
    };
    let archive = trash.join(uuid::Uuid::new_v4().to_string());
    std::fs::create_dir(&archive).map_err(|e| e.to_string())?;
    std::fs::write(
        archive.join("instance.json"),
        serde_json::to_vec_pretty(deleted).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("No se pudo guardar la copia de recuperación: {}", e))?;
    let moved = source.exists();
    if moved {
        std::fs::rename(&source, archive.join("data"))
            .map_err(|e| format!("No se pudo mover la instancia; cierra Minecraft: {}", e))?;
    }
    let mut moved_snapshots = false;
    if has_snapshots {
        match std::fs::rename(&snapshots, archive.join("snapshots")) {
            Ok(()) => moved_snapshots = true,
            Err(error) => {
                if moved {
                    std::fs::rename(archive.join("data"), &source).map_err(|e| {
                        format!(
                            "{error}; conserva tus archivos en {} ({e}).",
                            archive.display()
                        )
                    })?;
                }
                return Err(format!(
                    "No se pudieron mover las copias de seguridad; cierra Minecraft: {error}"
                ));
            }
        }
    }
    if let Err(error) = write(&state_path, state_json) {
        if moved_snapshots {
            std::fs::rename(archive.join("snapshots"), &snapshots).map_err(|e| {
                format!(
                    "{error}; conserva tus copias en {} ({e}).",
                    archive.display()
                )
            })?;
        }
        if moved {
            std::fs::rename(archive.join("data"), &source).map_err(|e| {
                format!(
                    "{}; conserva tus archivos en {} ({}).",
                    error,
                    archive.display(),
                    e
                )
            })?;
        }
        return Err(error);
    }
    Ok(archive.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deletion_preserves_worlds_and_allows_empty_library() {
        let root =
            std::env::temp_dir().join(format!("openclient-delete-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("instances/demo/saves/world")).unwrap();
        std::fs::write(
            root.join("instances/demo/saves/world/level.dat"),
            b"world-data",
        )
        .unwrap();
        std::fs::write(
            root.join("launcher_state.json"),
            r#"{"instances":[{"id":"demo"}]}"#,
        )
        .unwrap();
        let next = r#"{"instances":[],"activeInstanceId":""}"#;
        let archive = delete_at(&root, "demo", next).unwrap();
        assert!(!root.join("instances/demo").exists());
        assert_eq!(
            std::fs::read(Path::new(&archive).join("data/saves/world/level.dat")).unwrap(),
            b"world-data"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("launcher_state.json")).unwrap(),
            next
        );
        assert!(delete_at(&root, "../outside", next).is_err());
        assert!(delete_at(&root, "demo", next).is_err());
    }
    #[test]
    fn deletion_moves_snapshots_to_trash_and_restores_them_on_failure() {
        let root =
            std::env::temp_dir().join(format!("openclient-delete-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("instances/demo")).unwrap();
        std::fs::create_dir_all(root.join("snapshots/demo/1-aaaaaaaa/files")).unwrap();
        std::fs::write(root.join("snapshots/demo/1-aaaaaaaa/meta.json"), b"meta").unwrap();
        std::fs::create_dir_all(root.join("snapshots/other/2-bbbbbbbb")).unwrap();
        let old = r#"{"instances":[{"id":"demo"},{"id":"other"}]}"#;
        std::fs::write(root.join("launcher_state.json"), old).unwrap();
        let next = r#"{"instances":[{"id":"other"}]}"#;
        assert!(delete_with_writer(&root, "demo", next, |_, _| Err("Disk full".into())).is_err());
        assert!(root.join("instances/demo").exists());
        assert!(root.join("snapshots/demo/1-aaaaaaaa/meta.json").is_file());

        let archive = delete_at(&root, "demo", next).unwrap();
        assert!(!root.join("snapshots/demo").exists());
        assert!(root.join("snapshots/other/2-bbbbbbbb").is_dir());
        assert_eq!(
            std::fs::read(Path::new(&archive).join("snapshots/1-aaaaaaaa/meta.json")).unwrap(),
            b"meta"
        );
        let trashed: Vec<_> = std::fs::read_dir(root.join("trash"))
            .unwrap()
            .map(|entry| entry.unwrap().path().join("snapshots/1-aaaaaaaa/meta.json"))
            .collect();
        assert!(trashed.iter().any(|path| path.is_file()));
    }

    #[test]
    fn invalid_state_does_not_move_instance() {
        let root =
            std::env::temp_dir().join(format!("openclient-delete-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("instances/demo")).unwrap();
        assert!(delete_at(&root, "demo", r#"{"instances":[{"id":"demo"}]}"#).is_err());
        assert!(root.join("instances/demo").exists());
    }

    #[test]
    fn failed_save_restores_instance_and_keeps_original_library() {
        let root =
            std::env::temp_dir().join(format!("openclient-delete-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("instances/demo")).unwrap();
        std::fs::write(root.join("instances/demo/options.txt"), b"settings").unwrap();
        let old = r#"{"instances":[{"id":"demo"},{"id":"other"}]}"#;
        std::fs::write(root.join("launcher_state.json"), old).unwrap();
        let result = delete_with_writer(
            &root,
            "demo",
            r#"{"instances":[{"id":"other"}]}"#,
            |_, _| Err("Disk full".into()),
        );
        assert!(result.is_err());
        assert_eq!(
            std::fs::read(root.join("instances/demo/options.txt")).unwrap(),
            b"settings"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("launcher_state.json")).unwrap(),
            old
        );
    }
}
