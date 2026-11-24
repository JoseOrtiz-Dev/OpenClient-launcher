use crate::directory_install::{copy_tree, PreparedDirectory};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};

const MANIFEST: &str = "openclient-instance.json";
pub(crate) const DIRECTORIES: &[&str] = &[
    "saves",
    "mods",
    "config",
    "defaultconfigs",
    "screenshots",
    "resourcepacks",
    "shaderpacks",
    "datapacks",
    "kubejs",
];
pub(crate) const FILES: &[&str] = &["options.txt", "servers.dat"];

fn canonical_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err("Selecciona una carpeta local válida.".into());
    }
    path.canonicalize().map_err(|error| error.to_string())
}

pub(crate) fn isolated_path(root: &Path, directory: &Path) -> Result<PathBuf, String> {
    let path = canonical_directory(directory)?;
    if path.starts_with(root) || root.starts_with(&path) {
        return Err("Elige una carpeta fuera del directorio de datos de OpenClient.".into());
    }
    Ok(path)
}

fn normal_item(path: &Path) -> Result<Option<fs::Metadata>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_symlink() || path.canonicalize().map_err(|e| e.to_string())? != path
    {
        return Err("La instancia contiene un enlace a otra ubicación.".into());
    }
    Ok(Some(metadata))
}

pub(crate) fn copy_game(source: &Path, destination: &Path) -> Result<(), String> {
    for name in DIRECTORIES {
        let from = source.join(name);
        if let Some(metadata) = normal_item(&from)? {
            if !metadata.is_dir() {
                return Err(format!("{name} no es una carpeta."));
            }
            let to = destination.join(name);
            fs::create_dir(&to).map_err(|error| error.to_string())?;
            copy_tree(&from, &to)?;
        }
    }
    for name in FILES {
        let from = source.join(name);
        if let Some(metadata) = normal_item(&from)? {
            if !metadata.is_file() {
                return Err(format!("{name} no es un archivo."));
            }
            fs::copy(&from, destination.join(name)).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub fn export(
    root: &Path,
    id: &str,
    destination: &Path,
    metadata: &Value,
) -> Result<PathBuf, String> {
    if !uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id) {
        return Err("Identificador de instancia inválido.".into());
    }
    let root = canonical_directory(root)?;
    let destination = isolated_path(&root, destination)?;
    let source = root.join("instances").join(id);
    let target = destination.join(format!("OpenClient-{}", uuid::Uuid::new_v4()));
    let staged = PreparedDirectory::new(&target, false)?;
    let game = staged.path.join("game");
    fs::create_dir(&game).map_err(|error| error.to_string())?;
    if let Some(source_metadata) = normal_item(&source)? {
        if !source_metadata.is_dir() {
            return Err("La instancia no tiene una carpeta válida.".into());
        }
        copy_game(&source, &game)?;
    }
    let manifest =
        serde_json::json!({"format": "openclient-instance", "version": 1, "instance": metadata});
    fs::write(
        staged.path.join(MANIFEST),
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::rename(&staged.path, &target)
        .map_err(|error| format!("No se pudo guardar la exportación: {error}"))?;
    Ok(target)
}

pub fn read(root: &Path, source: &Path) -> Result<(PathBuf, Value), String> {
    let root = canonical_directory(root)?;
    let source = isolated_path(&root, source)?;
    let manifest = source.join(MANIFEST);
    let metadata =
        normal_item(&manifest)?.ok_or("No se encontró openclient-instance.json en la carpeta.")?;
    if !metadata.is_file() || metadata.len() > 512 * 1024 {
        return Err("El manifiesto de la instancia no es válido.".into());
    }
    let value: Value =
        serde_json::from_slice(&fs::read(manifest).map_err(|error| error.to_string())?)
            .map_err(|_| "No se pudo leer el manifiesto de OpenClient.")?;
    if value["format"] != "openclient-instance"
        || value["version"] != 1
        || !value["instance"].is_object()
    {
        return Err("Este formato de instancia no es compatible.".into());
    }
    let game = source.join("game");
    if !normal_item(&game)?.is_some_and(|entry| entry.is_dir()) {
        return Err("La exportación no contiene la carpeta game.".into());
    }
    Ok((game, value["instance"].clone()))
}

pub fn import(root: &Path, game: &Path, id: uuid::Uuid, state_json: &str) -> Result<(), String> {
    let root = canonical_directory(root)?;
    let game = isolated_path(&root, game)?;
    let parent = root.join("instances");
    if let Some(existing) = normal_item(&parent)? {
        if !existing.is_dir() {
            return Err("La biblioteca no es una carpeta.".into());
        }
    }
    let target = parent.join(id.to_string());
    crate::instance_storage::publish_instance(&root, &target, state_json, |staging| {
        copy_game(&game, &staging.path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openclient-{label}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Biblioteca con una instancia completa más archivos que nunca deben viajar.
    fn library(id: &str) -> (PathBuf, PathBuf) {
        let base = temp_dir("portable");
        let root = base.join("datos");
        let game = root.join("instances").join(id);
        for (path, content) in [
            ("saves/Mundo/level.dat", "nivel"),
            ("saves/Mundo/region/r.0.0.mca", "region"),
            ("mods/sodium.jar", "jar"),
            ("config/sodium/opciones.json", "{}"),
            ("resourcepacks/pack.zip", "zip"),
            ("shaderpacks/sombras.zip", "sombras"),
            ("options.txt", "fov:90"),
            ("servers.dat", "servidores"),
            ("logs/openclient-latest.log", "registro"),
            ("crash-reports/crash.txt", "fallo"),
            ("token.txt", "secreto"),
        ] {
            fs::create_dir_all(game.join(path).parent().unwrap()).unwrap();
            fs::write(game.join(path), content).unwrap();
        }
        fs::create_dir_all(game.join("screenshots")).unwrap();
        (base, root)
    }

    fn files_under(directory: &Path) -> Vec<String> {
        fn walk(base: &Path, directory: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(base, &path, out);
                } else {
                    out.push(path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let mut out = Vec::new();
        walk(directory, directory, &mut out);
        out.sort();
        out
    }

    /// Enlace simbólico; en Windows, sin privilegios, una unión de directorios.
    fn symlink_dir(target: &Path, link: &Path) -> bool {
        #[cfg(windows)]
        {
            if std::os::windows::fs::symlink_dir(target, link).is_ok() {
                return true;
            }
            let shell = std::env::var_os("SystemRoot")
                .map_or_else(|| PathBuf::from("cmd"), |root| PathBuf::from(root).join("System32").join("cmd.exe"));
            std::process::Command::new(shell)
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .output()
                .is_ok_and(|output| output.status.success())
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
    }

    #[test]
    fn export_then_import_keeps_game_files_and_leaves_out_everything_else() {
        let id = uuid::Uuid::new_v4().to_string();
        let (base, root) = library(&id);
        let destination = base.join("destino");
        fs::create_dir_all(&destination).unwrap();
        let metadata = json!({"name": "Mi mundo", "mcVersion": "1.21.4", "category": "fabric"});

        let folder = export(&root, &id, &destination, &metadata).unwrap();
        assert!(folder.file_name().unwrap().to_string_lossy().starts_with("OpenClient-"));
        let manifest: Value = serde_json::from_slice(&fs::read(folder.join(MANIFEST)).unwrap()).unwrap();
        assert_eq!(manifest["format"], "openclient-instance");
        assert_eq!(manifest["version"], 1);
        assert_eq!(manifest["instance"], metadata);
        let exported = files_under(&folder.join("game"));
        assert_eq!(
            exported,
            [
                "config/sodium/opciones.json", "mods/sodium.jar", "options.txt", "resourcepacks/pack.zip",
                "saves/Mundo/level.dat", "saves/Mundo/region/r.0.0.mca", "servers.dat", "shaderpacks/sombras.zip",
            ]
        );
        // Las carpetas vacías de la lista también viajan.
        assert!(folder.join("game/screenshots").is_dir());
        // No queda la carpeta temporal de preparación.
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 1);

        let (game, read_metadata) = read(&root, &folder).unwrap();
        assert_eq!(read_metadata, metadata);
        assert!(game.ends_with("game"));
        let copy = uuid::Uuid::new_v4();
        let state = r#"{"instances":[{"id":"copia"}]}"#;
        import(&root, &game, copy, state).unwrap();
        let imported = root.join("instances").join(copy.to_string());
        assert_eq!(files_under(&imported), exported);
        assert_eq!(fs::read(imported.join("saves/Mundo/level.dat")).unwrap(), b"nivel");
        assert_eq!(fs::read(imported.join("options.txt")).unwrap(), b"fov:90");
        assert!(imported.join("screenshots").is_dir());
        assert_eq!(fs::read_to_string(root.join("launcher_state.json")).unwrap(), state);
        // La instancia original no cambia.
        assert!(root.join("instances").join(&id).join("token.txt").is_file());
        // Importar sobre un identificador ya usado falla sin tocar la copia.
        assert!(import(&root, &game, copy, state).is_err());
        assert_eq!(fs::read(imported.join("servers.dat")).unwrap(), b"servidores");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn exporting_an_instance_without_a_folder_produces_an_empty_game_directory() {
        let base = temp_dir("portable-empty");
        let root = base.join("datos");
        let destination = base.join("destino");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&destination).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let folder = export(&root, &id, &destination, &json!({"name": "vacía"})).unwrap();
        assert_eq!(fs::read_dir(folder.join("game")).unwrap().count(), 0);
        assert_eq!(read(&root, &folder).unwrap().1["name"], "vacía");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn export_rejects_bad_identifiers_and_destinations_inside_or_around_the_data_folder() {
        let id = uuid::Uuid::new_v4().to_string();
        let (base, root) = library(&id);
        let destination = base.join("destino");
        fs::create_dir_all(&destination).unwrap();
        let metadata = json!({});
        for bad in [id.to_uppercase(), "../datos".into(), "no-es-uuid".into(), String::new(), format!("{id}/..")] {
            assert_eq!(
                export(&root, &bad, &destination, &metadata).unwrap_err(),
                "Identificador de instancia inválido.",
                "{bad}"
            );
        }
        let isolated = "Elige una carpeta fuera del directorio de datos de OpenClient.";
        for inside in [root.clone(), root.join("instances"), root.join("instances").join(&id), base.clone()] {
            assert_eq!(export(&root, &id, &inside, &metadata).unwrap_err(), isolated, "{inside:?}");
        }
        for invalid in [PathBuf::from("relativa"), base.join("no-existe"), root.join("instances").join(&id).join("options.txt")] {
            assert_eq!(
                export(&root, &id, &invalid, &metadata).unwrap_err(),
                "Selecciona una carpeta local válida.",
                "{invalid:?}"
            );
        }
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn a_broken_instance_aborts_the_export_without_leaving_partial_folders() {
        let id = uuid::Uuid::new_v4().to_string();
        let (base, root) = library(&id);
        let destination = base.join("destino");
        fs::create_dir_all(&destination).unwrap();
        let game = root.join("instances").join(&id);
        fs::remove_dir_all(game.join("mods")).unwrap();
        fs::write(game.join("mods"), b"no soy una carpeta").unwrap();
        assert_eq!(export(&root, &id, &destination, &json!({})).unwrap_err(), "mods no es una carpeta.");
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        fs::remove_file(game.join("mods")).unwrap();
        fs::remove_file(game.join("options.txt")).unwrap();
        fs::create_dir(game.join("options.txt")).unwrap();
        assert_eq!(export(&root, &id, &destination, &json!({})).unwrap_err(), "options.txt no es un archivo.");
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn links_to_other_locations_are_refused_on_export_and_import() {
        let id = uuid::Uuid::new_v4().to_string();
        let (base, root) = library(&id);
        let destination = base.join("destino");
        let outside = base.join("fuera");
        fs::create_dir_all(&destination).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secreto.txt"), b"no copiar").unwrap();
        let game = root.join("instances").join(&id);
        fs::remove_dir_all(game.join("config")).unwrap();
        if !symlink_dir(&outside, &game.join("config")) {
            eprintln!("sin permiso para crear enlaces simbólicos; se omite la comprobación");
            fs::remove_dir_all(base).unwrap();
            return;
        }
        let error = export(&root, &id, &destination, &json!({})).unwrap_err();
        assert_eq!(error, "La instancia contiene un enlace a otra ubicación.");
        assert_eq!(fs::read_dir(&destination).unwrap().count(), 0);

        // Una carpeta exportada a mano con `game` o una subcarpeta enlazadas tampoco se importa.
        let package = base.join("paquete");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join(MANIFEST),
            json!({"format": "openclient-instance", "version": 1, "instance": {}}).to_string(),
        )
        .unwrap();
        assert!(symlink_dir(&outside, &package.join("game")));
        assert!(read(&root, &package).unwrap_err().contains("enlace"));
        #[cfg(unix)]
        fs::remove_file(package.join("game")).unwrap();
        #[cfg(windows)]
        fs::remove_dir(package.join("game")).unwrap();
        fs::create_dir(package.join("game")).unwrap();
        assert!(symlink_dir(&outside, &package.join("game").join("saves")));
        let (linked_game, _) = read(&root, &package).unwrap();
        let copy = uuid::Uuid::new_v4();
        assert!(import(&root, &linked_game, copy, r#"{"instances":[]}"#).is_err());
        assert!(!root.join("instances").join(copy.to_string()).exists());
        assert!(!root.join("launcher_state.json").exists());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn read_validates_manifest_game_folder_and_location() {
        let base = temp_dir("portable-read");
        let root = base.join("datos");
        fs::create_dir_all(root.join("instances")).unwrap();
        let package = base.join("paquete");
        fs::create_dir_all(&package).unwrap();
        let write = |text: String| fs::write(package.join(MANIFEST), text).unwrap();
        let valid = || json!({"format": "openclient-instance", "version": 1, "instance": {"name": "x"}}).to_string();

        assert_eq!(
            read(&root, &package).unwrap_err(),
            "No se encontró openclient-instance.json en la carpeta."
        );
        write("{no json".into());
        assert_eq!(read(&root, &package).unwrap_err(), "No se pudo leer el manifiesto de OpenClient.");
        for manifest in [
            json!({"format": "otro", "version": 1, "instance": {}}),
            json!({"format": "openclient-instance", "version": 2, "instance": {}}),
            json!({"format": "openclient-instance", "version": 1, "instance": []}),
            json!({"format": "openclient-instance", "version": 1}),
        ] {
            write(manifest.to_string());
            assert_eq!(read(&root, &package).unwrap_err(), "Este formato de instancia no es compatible.", "{manifest}");
        }
        write(format!(r#"{{"relleno":"{}"}}"#, "a".repeat(512 * 1024)));
        assert_eq!(read(&root, &package).unwrap_err(), "El manifiesto de la instancia no es válido.");
        write(valid());
        assert_eq!(read(&root, &package).unwrap_err(), "La exportación no contiene la carpeta game.");
        fs::write(package.join("game"), b"archivo").unwrap();
        assert_eq!(read(&root, &package).unwrap_err(), "La exportación no contiene la carpeta game.");
        fs::remove_file(package.join("game")).unwrap();
        fs::create_dir(package.join("game")).unwrap();
        assert_eq!(read(&root, &package).unwrap().1["name"], "x");
        // Una exportación dentro de la biblioteca (o la propia biblioteca) se rechaza.
        let inside = root.join("instances").join("paquete");
        fs::create_dir_all(inside.join("game")).unwrap();
        fs::write(inside.join(MANIFEST), valid()).unwrap();
        let isolated = "Elige una carpeta fuera del directorio de datos de OpenClient.";
        assert_eq!(read(&root, &inside).unwrap_err(), isolated);
        assert_eq!(read(&root, &root).unwrap_err(), isolated);
        assert_eq!(read(&root, &base).unwrap_err(), isolated);
        assert_eq!(read(&root, Path::new("relativa")).unwrap_err(), "Selecciona una carpeta local válida.");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn import_refuses_sources_inside_the_library_and_invalid_libraries() {
        let base = temp_dir("portable-import");
        let root = base.join("datos");
        fs::create_dir_all(root.join("instances/otra/mods")).unwrap();
        let id = uuid::Uuid::new_v4();
        let state = r#"{"instances":[]}"#;
        assert_eq!(
            import(&root, &root.join("instances/otra"), id, state).unwrap_err(),
            "Elige una carpeta fuera del directorio de datos de OpenClient."
        );
        let game = base.join("game");
        fs::create_dir_all(game.join("mods")).unwrap();
        fs::write(game.join("mods/a.jar"), b"a").unwrap();
        // Estado de biblioteca inválido: no queda ninguna carpeta publicada.
        assert!(import(&root, &game, id, "no json").is_err());
        assert!(!root.join("instances").join(id.to_string()).exists());
        // `instances` no puede ser un archivo.
        let other = base.join("otra-biblioteca");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("instances"), b"archivo").unwrap();
        assert_eq!(import(&other, &game, id, state).unwrap_err(), "La biblioteca no es una carpeta.");
        // Importar crea `instances` cuando todavía no existe.
        let fresh = base.join("nueva");
        fs::create_dir_all(&fresh).unwrap();
        import(&fresh, &game, id, state).unwrap();
        assert_eq!(fs::read(fresh.join("instances").join(id.to_string()).join("mods/a.jar")).unwrap(), b"a");
        fs::remove_dir_all(base).unwrap();
    }
}
