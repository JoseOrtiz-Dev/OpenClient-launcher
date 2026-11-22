//! Espacio que usa el launcher y limpieza de lo que se puede volver a descargar.

use serde_json::{json, Value};
use std::{fs, path::Path};

const GAME_FILES: [&str; 3] = ["libraries", "assets", "versions"];

fn plain_directory(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    metadata.is_dir() && !metadata.file_type().is_symlink()
}

fn size_of(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.file_type().is_symlink() {
        return 0;
    }
    if !metadata.is_dir() {
        return metadata.len();
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return 0;
        }
    }
    fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| size_of(&entry.path()))
                .sum()
        })
        .unwrap_or(0)
}

pub fn usage(root: &Path) -> Value {
    let game_files: u64 = GAME_FILES.iter().map(|name| size_of(&root.join(name))).sum();
    json!({
        "gameFiles": game_files,
        "runtimes": size_of(&root.join("runtimes")),
        "trash": size_of(&root.join("trash")),
        "instances": size_of(&root.join("instances")),
    })
}

fn clear_directory(root: &Path, name: &str) -> Result<(), String> {
    let target = root.join(name);
    if fs::symlink_metadata(&target).is_err() {
        return Ok(());
    }
    if !plain_directory(&target) {
        return Err(format!("«{name}» no es una carpeta normal; no se tocó."));
    }
    let canonical = target.canonicalize().map_err(|error| error.to_string())?;
    let base = root.canonicalize().map_err(|error| error.to_string())?;
    if canonical.parent() != Some(base.as_path()) {
        return Err(format!("«{name}» apunta fuera del launcher; no se tocó."));
    }
    fs::remove_dir_all(&canonical).map_err(|error| format!("No se pudo limpiar «{name}»: {error}"))
}

/// `game_files` borra librerías, recursos y perfiles de versión, que se vuelven a descargar al jugar.
/// `trash` borra de forma permanente las instancias retiradas.
pub fn clear(root: &Path, kind: &str) -> Result<Value, String> {
    match kind {
        "game_files" => {
            for name in GAME_FILES {
                clear_directory(root, name)?;
            }
        }
        "trash" => clear_directory(root, "trash")?,
        _ => return Err("Tipo de limpieza desconocido.".into()),
    }
    Ok(usage(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("openclient-storage-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn usage_counts_and_clear_keeps_instances() {
        let root = scratch();
        for (directory, bytes) in [("libraries", 10), ("assets", 20), ("trash", 5), ("instances", 7)] {
            fs::create_dir_all(root.join(directory).join("nested")).unwrap();
            fs::write(root.join(directory).join("nested/file"), vec![0u8; bytes]).unwrap();
        }
        let before = usage(&root);
        assert_eq!(before["gameFiles"], 30);
        assert_eq!(before["trash"], 5);
        let after = clear(&root, "game_files").unwrap();
        assert_eq!(after["gameFiles"], 0);
        assert_eq!(after["instances"], 7);
        assert_eq!(after["trash"], 5);
        assert_eq!(clear(&root, "trash").unwrap()["trash"], 0);
        assert!(root.join("instances/nested/file").exists());
        assert!(clear(&root, "instances").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn junctions_are_neither_followed_nor_cleared() {
        let root = scratch();
        let outside = scratch();
        fs::write(outside.join("keep.txt"), b"keep").unwrap();
        let status = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(root.join("libraries"))
            .arg(&outside)
            .output()
            .unwrap()
            .status;
        if status.success() {
            assert_eq!(usage(&root)["gameFiles"], 0);
            assert!(clear(&root, "game_files").is_err());
            assert!(outside.join("keep.txt").exists());
            let _ = fs::remove_dir(root.join("libraries"));
        }
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
