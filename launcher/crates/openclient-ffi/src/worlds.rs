//! Acciones de copias de seguridad de mundos: `world_list`, `world_backup`, `world_restore`,
//! `world_backup_delete`, y la copia automática previa a actualizar un modpack.

use openclient_core::{
    world_backup::{self, BackupInfo, Kind, Outcome, WorldInfo, AUTO_LIMIT},
    GameContext,
};
use serde_json::{json, Value};

fn backup_json(info: &BackupInfo) -> Value {
    json!({
        "id": info.id, "auto": info.kind == Kind::Auto, "created": info.created,
        "bytes": info.bytes, "worlds": info.worlds
    })
}

fn world_json(info: &WorldInfo) -> Value {
    json!({"name": info.name, "bytes": info.bytes, "modified": info.modified})
}

fn target(request: &Value) -> Result<(&std::path::Path, String), String> {
    let (root, instance) = crate::game::selection(request)?;
    let id = instance["id"].as_str().ok_or("Identificador de instancia inválido.")?;
    Ok((root, id.to_owned()))
}

/// `world_list` `{root,id}` → `{"worlds":[{name,bytes,modified}],"backups":[{id,auto,created,bytes,worlds}]}`.
/// Solo lectura.
pub fn list(request: &Value) -> Result<Value, String> {
    let (root, id) = target(request)?;
    Ok(json!({
        "worlds": world_backup::worlds(root, &id)?.iter().map(world_json).collect::<Vec<_>>(),
        "backups": world_backup::backups(root, &id)?.iter().map(backup_json).collect::<Vec<_>>(),
    }))
}

/// `world_backup` `{root,id,worlds?:[nombre]}` → `{"backup":{…}}`. Sin `worlds` copia todos.
/// Operación registrada y cancelable.
pub fn backup(request: &Value, ctx: &GameContext) -> Result<Value, String> {
    let (_, id) = target(request)?;
    let names = match &request["worlds"] {
        Value::Null => None,
        Value::Array(items) if items.len() <= 200 => Some(
            items
                .iter()
                .map(|item| item.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or("Lista de mundos inválida.")?,
        ),
        _ => return Err("Lista de mundos inválida.".into()),
    };
    ctx.progress("backup", 0, "Copiando mundos");
    match world_backup::create(&ctx.root, &id, names.as_deref(), Kind::Manual, u64::MAX, ctx)? {
        Outcome::Created(info) => Ok(json!({"backup": backup_json(&info)})),
        Outcome::NoWorlds => Err("Esta instancia todavía no tiene mundos.".into()),
        Outcome::TooLarge(_) => Err("Los mundos son demasiado grandes para copiarlos.".into()),
    }
}

/// `world_restore` `{root,id,backup}` → `{"restored":{…},"previous":{…}|null}`.
/// Antes guarda una copia automática de lo que reemplaza. Operación registrada y cancelable.
pub fn restore(request: &Value, ctx: &GameContext) -> Result<Value, String> {
    let (_, id) = target(request)?;
    let backup = request["backup"].as_str().ok_or("Falta la copia de seguridad.")?;
    let done = world_backup::restore(&ctx.root, &id, backup, ctx)?;
    Ok(json!({
        "restored": backup_json(&done.restored),
        "previous": done.previous.as_ref().map(backup_json),
    }))
}

/// `world_backup_delete` `{root,id,backup}` → `{"backups":[…]}`.
pub fn delete(request: &Value) -> Result<Value, String> {
    let (root, id) = target(request)?;
    let backup = request["backup"].as_str().ok_or("Falta la copia de seguridad.")?;
    world_backup::delete(root, &id, backup)?;
    Ok(json!({"backups": world_backup::backups(root, &id)?.iter().map(backup_json).collect::<Vec<_>>()}))
}

/// Copia automática antes de cambiar el contenido de una instancia. Devuelve
/// `{"worldBackup":"<id>"}`, `{"worldBackupSkipped":<bytes>}` (mundos de más de 2 GiB) o `{}`
/// si no hay mundos. Un fallo cancela la operación: no se cambia nada sin respaldo.
pub fn auto(ctx: &GameContext, id: &str) -> Result<Value, String> {
    ctx.progress("backup", 0, "Guardando copia de los mundos");
    Ok(match world_backup::create(&ctx.root, id, None, Kind::Auto, AUTO_LIMIT, ctx)? {
        Outcome::Created(info) => json!({"worldBackup": info.id}),
        Outcome::TooLarge(bytes) => json!({"worldBackupSkipped": bytes}),
        Outcome::NoWorlds => json!({}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn library() -> (std::path::PathBuf, String) {
        let root = std::env::temp_dir().join(format!("openclient-worlds-{}", uuid::Uuid::new_v4()));
        let id = uuid::Uuid::new_v4().to_string();
        let saves = root.join("instances").join(&id).join("saves/Mundo");
        std::fs::create_dir_all(&saves).unwrap();
        std::fs::write(saves.join("level.dat"), b"uno").unwrap();
        let state = json!({"instances": [{"id": id, "name": "P", "mcVersion": "1.21.4", "category": "fabric"}]});
        std::fs::write(root.join("launcher_state.json"), state.to_string()).unwrap();
        (root, id)
    }

    fn ctx(root: &Path) -> GameContext {
        GameContext::new(root.to_owned(), "test", |_, _| Ok(()))
    }

    #[test]
    fn list_backup_restore_and_delete() {
        let (root, id) = library();
        let request = |extra: Value| {
            let mut value = json!({"root": root, "id": id});
            value.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
            value
        };
        let listed = list(&request(json!({}))).unwrap();
        assert_eq!(listed["worlds"][0]["name"], "Mundo");
        assert_eq!(listed["backups"], json!([]));

        let made = backup(&request(json!({"worlds": ["Mundo"]})), &ctx(&root)).unwrap();
        let backup_id = made["backup"]["id"].as_str().unwrap().to_owned();
        assert_eq!(made["backup"]["auto"], false);

        std::fs::write(root.join("instances").join(&id).join("saves/Mundo/level.dat"), b"dos").unwrap();
        let restored = restore(&request(json!({"backup": backup_id})), &ctx(&root)).unwrap();
        assert_eq!(restored["restored"]["id"], backup_id);
        assert_eq!(restored["previous"]["auto"], true);
        let level = root.join("instances").join(&id).join("saves/Mundo/level.dat");
        assert_eq!(std::fs::read(level).unwrap(), b"uno");

        let left = delete(&request(json!({"backup": backup_id}))).unwrap();
        assert_eq!(left["backups"].as_array().unwrap().len(), 1);
        for bad in [json!({"worlds": "x"}), json!({"worlds": [1]}), json!({"worlds": ["../x"]})] {
            assert!(backup(&request(bad), &ctx(&root)).is_err());
        }
        assert!(list(&json!({"root": root, "id": "../x"})).is_err());
        assert!(delete(&request(json!({"backup": "../x"}))).is_err());
        assert!(auto(&ctx(&root), &id).unwrap()["worldBackup"].as_str().is_some());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn instance_without_worlds_reports_clearly() {
        let (root, id) = library();
        std::fs::remove_dir_all(root.join("instances").join(&id).join("saves")).unwrap();
        let error = backup(&json!({"root": root, "id": id}), &ctx(&root)).unwrap_err();
        assert!(error.contains("no tiene mundos"));
        assert_eq!(auto(&ctx(&root), &id).unwrap(), json!({}));
        std::fs::remove_dir_all(root).unwrap();
    }
}
