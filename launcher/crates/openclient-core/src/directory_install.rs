//! Prepare downloads beside the live directory; only publish a complete tree.
use std::path::{Path, PathBuf};

fn previous(target: &Path) -> Result<PathBuf, String> {
    let name = target
        .file_name()
        .ok_or("Directorio de instalación inválido")?;
    let mut backup = name.to_os_string();
    backup.push(".openclient-previous");
    Ok(target.with_file_name(backup))
}

pub fn recover(target: &Path) -> Result<(), String> {
    let backup = previous(target)?;
    for directory in [target, backup.as_path()] {
        if let Ok(metadata) = std::fs::symlink_metadata(directory) {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    return Err("La instalación apunta a un enlace de carpeta; no se modificaron sus datos.".into());
                }
            }
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("La instalación no es una carpeta normal.".into());
            }
        }
    }
    if backup.exists() && !target.exists() {
        std::fs::rename(&backup, target)
            .map_err(|e| format!("No se pudo recuperar la instalación anterior: {}", e))?;
    }
    Ok(())
}

pub struct PreparedDirectory {
    target: PathBuf,
    pub path: PathBuf,
}

pub(crate) fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    for entry in std::fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let metadata = std::fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err("La carpeta contiene un enlace; no se puede copiar.".into());
            }
        }
        if metadata.file_type().is_symlink() {
            return Err("No se pueden copiar enlaces durante la instalación.".into());
        }
        let dest = destination.join(entry.file_name());
        if metadata.is_dir() {
            std::fs::create_dir(&dest).map_err(|e| e.to_string())?;
            copy_tree(&entry.path(), &dest)?;
        } else if metadata.is_file() {
            std::fs::copy(entry.path(), dest).map_err(|e| e.to_string())?;
        } else {
            return Err("Archivo no compatible en la carpeta de mods.".into());
        }
    }
    Ok(())
}

impl PreparedDirectory {
    pub(crate) fn copy_from(&self, source: &Path) -> Result<(), String> {
        copy_tree(source, &self.path)
    }

    pub fn new(target: &Path, keep_existing: bool) -> Result<Self, String> {
        recover(target)?;
        let parent = target.parent().ok_or("Carpeta de instalación inválida")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let path = parent.join(format!(".openclient-stage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        let prepared = Self {
            target: target.to_path_buf(),
            path,
        };
        if keep_existing && target.exists() {
            copy_tree(target, &prepared.path)?;
        }
        Ok(prepared)
    }

    pub fn commit(self) -> Result<(), String> {
        let backup = previous(&self.target)?;
        if backup.exists() {
            std::fs::remove_dir_all(&backup)
                .map_err(|e| format!("No se pudo retirar el respaldo de la instalación: {}", e))?;
        }
        let moved = self.target.exists();
        if moved {
            std::fs::rename(&self.target, &backup)
                .map_err(|e| format!("Cierra Minecraft o Java antes de actualizar: {}", e))?;
        }
        if let Err(error) = std::fs::rename(&self.path, &self.target) {
            if moved {
                std::fs::rename(&backup, &self.target).map_err(|restore| {
                    format!(
                        "No se pudo instalar ({error}) ni restaurar ({restore}); conserva {}.",
                        backup.display()
                    )
                })?;
            }
            return Err(format!("No se pudo publicar la instalación: {}", error));
        }
        // The new directory is complete. A locked backup can be cleaned on the
        // next successful install without reporting the committed one as failed.
        if moved {
            let _ = std::fs::remove_dir_all(backup);
        }
        Ok(())
    }
}

impl Drop for PreparedDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_preparation_preserves_live_files_and_success_preserves_user_mods() {
        let root =
            std::env::temp_dir().join(format!("openclient-stage-test-{}", uuid::Uuid::new_v4()));
        let target = root.join("mods");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("user.jar"), b"user").unwrap();
        {
            let staged = PreparedDirectory::new(&target, true).unwrap();
            std::fs::write(staged.path.join("download.jar"), b"incomplete").unwrap();
        }
        assert!(!target.join("download.jar").exists());
        assert_eq!(std::fs::read(target.join("user.jar")).unwrap(), b"user");
        let staged = PreparedDirectory::new(&target, true).unwrap();
        std::fs::write(staged.path.join("download.jar"), b"complete").unwrap();
        staged.commit().unwrap();
        assert_eq!(
            std::fs::read(target.join("download.jar")).unwrap(),
            b"complete"
        );
        assert_eq!(std::fs::read(target.join("user.jar")).unwrap(), b"user");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn interrupted_directory_swap_restores_previous_installation() {
        let root =
            std::env::temp_dir().join(format!("openclient-recover-test-{}", uuid::Uuid::new_v4()));
        let target = root.join("java");
        let backup = previous(&target).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::write(backup.join("java.exe"), b"old").unwrap();
        recover(&target).unwrap();
        assert_eq!(std::fs::read(target.join("java.exe")).unwrap(), b"old");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_publish_restores_original_directory() {
        let root =
            std::env::temp_dir().join(format!("openclient-publish-test-{}", uuid::Uuid::new_v4()));
        let target = root.join("mods");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("old.jar"), b"original").unwrap();
        let staged = PreparedDirectory::new(&target, false).unwrap();
        std::fs::remove_dir(&staged.path).unwrap();
        assert!(staged.commit().is_err());
        assert_eq!(std::fs::read(target.join("old.jar")).unwrap(), b"original");
        std::fs::remove_dir_all(root).unwrap();
    }
}
