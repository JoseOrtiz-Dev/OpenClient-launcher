use openclient_core::{hooks, storage};
use serde_json::{json, Map, Value};
use std::path::Path;

const ARGUMENT_LIMIT: usize = 512;
const NOTES_LIMIT: usize = 2000;

fn text<'a>(request: &'a Value, key: &str) -> Result<Option<&'a str>, String> {
    match request.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        Some(_) => Err(format!("El campo {key} no es válido.")),
    }
}

fn java_path(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(String::new());
    }
    let path = Path::new(value);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !path.is_absolute()
        || !matches!(name.as_str(), "java.exe" | "javaw.exe" | "java")
        || !path.is_file()
        || value.chars().any(char::is_control)
    {
        return Err("Elige el ejecutable de una instalación de Java.".into());
    }
    Ok(value.to_owned())
}

/// Valida todos los campos recibidos y los escribe solo si ninguno falla.
pub fn apply(entry: &mut Value, request: &Value) -> Result<(), String> {
    let mut fields = Map::new();
    if let Some(value) = text(request, "javaPath")? {
        fields.insert("javaPath".into(), json!(java_path(value)?));
    }
    if let Some(value) = text(request, "jvmArgs")? {
        let value = value.trim();
        if value.chars().count() > ARGUMENT_LIMIT || value.chars().any(char::is_control) {
            return Err(format!(
                "Los argumentos JVM admiten hasta {ARGUMENT_LIMIT} caracteres."
            ));
        }
        fields.insert("jvmArgs".into(), json!(value));
    }
    if let Some(value) = request.get("useGlobalJvmArgs") {
        let flag = value
            .as_bool()
            .ok_or("El campo useGlobalJvmArgs no es válido.")?;
        fields.insert("useGlobalJvmArgs".into(), json!(flag));
    }
    if request.get("windowWidth").is_some() || request.get("windowHeight").is_some() {
        let width = request["windowWidth"].as_u64().ok_or("Ancho inválido.")?;
        let height = request["windowHeight"].as_u64().ok_or("Alto inválido.")?;
        let none = width == 0 && height == 0;
        if !none && !((320..=7680).contains(&width) && (240..=4320).contains(&height)) {
            return Err("La ventana debe medir entre 320×240 y 7680×4320.".into());
        }
        fields.insert("windowWidth".into(), json!(width));
        fields.insert("windowHeight".into(), json!(height));
    }
    if let Some(value) = text(request, "notes")? {
        if value.chars().count() > NOTES_LIMIT
            || value
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err(format!("Las notas admiten hasta {NOTES_LIMIT} caracteres."));
        }
        fields.insert("notes".into(), json!(value));
    }
    if let Some(value) = text(request, "joinServer")? {
        fields.insert(
            "joinServer".into(),
            json!(openclient_core::game_launcher::server_address(value)?),
        );
    }
    for (key, label) in [
        ("preLaunch", "comando previo"),
        ("postExit", "comando posterior"),
        ("wrapper", "comando envoltorio"),
    ] {
        if let Some(value) = text(request, key)? {
            let command = hooks::validate(value).map_err(|error| format!("El {label}: {error}"))?;
            fields.insert(key.into(), json!(command));
        }
    }
    let entry = entry.as_object_mut().ok_or("La instancia no es válida.")?;
    entry.extend(fields);
    Ok(())
}

pub fn storage_operation(action: &str, request: &Value) -> Result<Value, String> {
    let root = Path::new(request["root"].as_str().ok_or("Falta la carpeta de datos.")?);
    if !root.is_absolute() || !root.is_dir() {
        return Err("Carpeta de datos inválida.".into());
    }
    if action == "storage_usage" {
        return Ok(storage::usage(root));
    }
    if crate::game::busy() {
        return Err("Cierra Minecraft antes de limpiar archivos.".into());
    }
    storage::clear(root, request["kind"].as_str().unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_fields_are_applied_together() {
        let mut entry = json!({"id": "x", "name": "n"});
        apply(
            &mut entry,
            &json!({"jvmArgs": " -Dx=1 ", "useGlobalJvmArgs": false, "windowWidth": 1280,
                    "windowHeight": 720, "notes": "línea\nsegunda", "preLaunch": "  tool.exe -a ",
                    "wrapper": "", "javaPath": ""}),
        )
        .unwrap();
        assert_eq!(entry["jvmArgs"], "-Dx=1");
        assert_eq!(entry["useGlobalJvmArgs"], false);
        assert_eq!(entry["windowWidth"], 1280);
        assert_eq!(entry["preLaunch"], "tool.exe -a");
        assert_eq!(entry["wrapper"], "");
        assert_eq!(entry["name"], "n");
    }

    #[test]
    fn one_invalid_field_leaves_the_instance_untouched() {
        let mut entry = json!({"id": "x", "jvmArgs": "-Dold=1"});
        for bad in [
            json!({"jvmArgs": "-Dnew=1", "preLaunch": "tool\u{7}"}),
            json!({"jvmArgs": "-Dnew=1", "windowWidth": 100, "windowHeight": 100}),
            json!({"jvmArgs": "-Dnew=1", "windowWidth": 1280}),
            json!({"jvmArgs": "-Dnew=1", "javaPath": "relativo/java.exe"}),
            json!({"jvmArgs": "-Dnew=1", "javaPath": "C:\\Windows\\System32\\cmd.exe"}),
            json!({"jvmArgs": "-Dnew=1", "notes": "a".repeat(NOTES_LIMIT + 1)}),
            json!({"jvmArgs": "-Dnew=1", "wrapper": "\"sin cerrar"}),
            json!({"jvmArgs": "-Dnew=1", "joinServer": "host:70000"}),
            json!({"jvmArgs": 7}),
        ] {
            assert!(apply(&mut entry, &bad).is_err(), "{bad}");
        }
        assert_eq!(entry, json!({"id": "x", "jvmArgs": "-Dold=1"}));
    }

    #[test]
    fn window_can_be_reset_to_the_global_size() {
        let mut entry = json!({"windowWidth": 1280, "windowHeight": 720});
        apply(&mut entry, &json!({"windowWidth": 0, "windowHeight": 0})).unwrap();
        assert_eq!(entry["windowWidth"], 0);
    }

    #[test]
    fn exported_metadata_never_carries_commands() {
        let entry = json!({
            "name": "n", "mcVersion": "1.21.4", "category": "vanilla", "installedContent": [],
            "preLaunch": "evil.exe", "postExit": "evil.exe", "wrapper": "evil.exe",
            "javaPath": "C:\\x\\java.exe", "jvmArgs": "-Dx=1", "notes": "n", "joinServer": "evil.example"
        });
        let metadata = crate::portable_metadata(&entry).unwrap();
        for key in ["preLaunch", "postExit", "wrapper", "javaPath", "jvmArgs", "notes", "joinServer"] {
            assert!(metadata.get(key).is_none(), "{key} viaja en la exportación");
        }
    }
}
