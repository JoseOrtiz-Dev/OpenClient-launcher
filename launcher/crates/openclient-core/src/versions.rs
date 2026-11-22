//! Catálogo de versiones de Minecraft y de los cargadores que las admiten.
//!
//! Cada fuente se consulta por separado: si una falla, su lista queda en `null`
//! y la interfaz no filtra por ese cargador. El resultado se guarda en
//! `cache/minecraft-versions.json` y se reutiliza durante seis horas.

use serde_json::{json, Value};
use std::{collections::HashSet, path::Path};

const MOJANG: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
const FABRIC: &str = "https://meta.fabricmc.net/v2/versions/game";
const QUILT: &str = "https://meta.quiltmc.org/v3/versions/game";
const FORGE: &str = "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const NEOFORGE: &str = "https://maven.neoforged.net/api/maven/versions/releases/net/neoforged/neoforge";
const FRESH_SECONDS: u64 = 6 * 3600;

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_ ".contains(&byte))
}

pub fn parse_mojang(manifest: &Value) -> Result<Vec<Value>, String> {
    let versions = manifest["versions"]
        .as_array()
        .ok_or("El manifiesto de Mojang no es válido.")?;
    Ok(versions
        .iter()
        .filter_map(|entry| {
            let id = entry["id"].as_str().filter(|id| valid_id(id))?;
            let kind = entry["type"].as_str()?;
            if !matches!(kind, "release" | "snapshot" | "old_beta" | "old_alpha") {
                return None;
            }
            let released = entry["releaseTime"].as_str().unwrap_or_default().get(..10).unwrap_or_default();
            Some(json!({"id": id, "type": kind, "released": released}))
        })
        .collect())
}

/// Fabric y Quilt publican la misma forma: `[{"version": "...", "stable": bool}]`.
pub fn parse_game_list(list: &Value) -> Option<Vec<String>> {
    Some(
        list.as_array()?
            .iter()
            .filter_map(|entry| entry["version"].as_str().filter(|id| valid_id(id)).map(str::to_owned))
            .collect(),
    )
}

pub fn parse_forge(promotions: &Value) -> Option<Vec<String>> {
    let mut seen = HashSet::new();
    Some(
        promotions["promos"]
            .as_object()?
            .keys()
            .filter_map(|key| key.rsplit_once('-').map(|(mc, _)| mc.to_owned()))
            .filter(|mc| valid_id(mc) && seen.insert(mc.clone()))
            .collect(),
    )
}

/// El mismo prefijo que usa el motor para elegir el instalador de NeoForge.
pub fn neoforge_prefix(mc: &str) -> String {
    let prefix = mc.strip_prefix("1.").unwrap_or(mc);
    if prefix.contains('.') {
        format!("{prefix}.")
    } else {
        format!("{prefix}.0.")
    }
}

pub fn parse_neoforge(maven: &Value, releases: &[Value]) -> Option<Vec<String>> {
    let builds: Vec<&str> = maven["versions"].as_array()?.iter().filter_map(Value::as_str).collect();
    Some(
        releases
            .iter()
            .filter(|entry| entry["type"] == "release")
            .filter_map(|entry| entry["id"].as_str())
            .filter(|mc| {
                let prefix = neoforge_prefix(mc);
                builds.iter().any(|build| build.starts_with(&prefix))
            })
            .map(str::to_owned)
            .collect(),
    )
}

async fn fetch(client: &reqwest::Client, url: &str) -> Option<Value> {
    client.get(url).send().await.ok()?.error_for_status().ok()?.json().await.ok()
}

async fn download(client: &reqwest::Client) -> Result<Value, String> {
    let (mojang, fabric, quilt, forge, neoforge) = tokio::join!(
        fetch(client, MOJANG),
        fetch(client, FABRIC),
        fetch(client, QUILT),
        fetch(client, FORGE),
        fetch(client, NEOFORGE)
    );
    let versions = parse_mojang(&mojang.ok_or("No se pudo consultar las versiones de Minecraft.")?)?;
    Ok(json!({
        "versions": versions,
        "fabric": fabric.as_ref().and_then(parse_game_list),
        "quilt": quilt.as_ref().and_then(parse_game_list),
        "forge": forge.as_ref().and_then(parse_forge),
        "neoforge": neoforge.as_ref().and_then(|maven| parse_neoforge(maven, &versions)),
        "fetchedAt": crate::session_time(),
    }))
}

pub fn catalog(root: &Path, refresh: bool) -> Result<Value, String> {
    let cache = root.join("cache").join("minecraft-versions.json");
    let cached = std::fs::read_to_string(&cache)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(|value| value["versions"].as_array().is_some_and(|list| !list.is_empty()));
    if let Some(value) = &cached {
        let age = crate::session_time().saturating_sub(value["fetchedAt"].as_u64().unwrap_or(0));
        if !refresh && age < FRESH_SECONDS {
            return Ok(value.clone());
        }
    }
    let client = reqwest::Client::builder()
        .https_only(true)
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("OpenClient/2.0")
        .build()
        .map_err(|error| error.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    match runtime.block_on(download(&client)) {
        Ok(fresh) => {
            let _ = std::fs::create_dir_all(cache.parent().unwrap_or(root));
            let _ = std::fs::write(&cache, fresh.to_string());
            Ok(fresh)
        }
        Err(error) => match cached {
            Some(mut stale) => {
                stale["offline"] = json!(true);
                Ok(stale)
            }
            None => Err(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mojang_manifest_keeps_known_types_and_valid_ids() {
        let manifest = json!({"versions": [
            {"id": "26.1", "type": "release", "releaseTime": "2026-03-24T10:00:00+00:00"},
            {"id": "26.1-snapshot-3", "type": "snapshot", "releaseTime": "2026-02-01T10:00:00+00:00"},
            {"id": "b1.7.3", "type": "old_beta", "releaseTime": "2011-07-08T00:00:00+00:00"},
            {"id": "../evil", "type": "release", "releaseTime": "2026-01-01T00:00:00+00:00"},
            {"id": "x", "type": "pending", "releaseTime": ""}
        ]});
        let versions = parse_mojang(&manifest).unwrap();
        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0], json!({"id": "26.1", "type": "release", "released": "2026-03-24"}));
        assert!(parse_mojang(&json!({})).is_err());
    }

    #[test]
    fn loader_lists_are_parsed_and_neoforge_matches_the_engine_prefix() {
        assert_eq!(
            parse_game_list(&json!([{"version": "1.21.11", "stable": true}, {"version": 3}])).unwrap(),
            vec!["1.21.11"]
        );
        let mut forge = parse_forge(&json!({"promos": {"1.20.1-latest": "47.3.0", "1.20.1-recommended": "47.2.0", "1.12.2-latest": "x"}})).unwrap();
        forge.sort();
        assert_eq!(forge, vec!["1.12.2", "1.20.1"]);
        let releases = vec![
            json!({"id": "1.21", "type": "release"}),
            json!({"id": "1.21.4", "type": "release"}),
            json!({"id": "1.21.5", "type": "release"}),
            json!({"id": "1.21.4-pre1", "type": "snapshot"}),
        ];
        let neoforge = parse_neoforge(&json!({"versions": ["21.0.167", "21.4.84"]}), &releases).unwrap();
        assert_eq!(neoforge, vec!["1.21", "1.21.4"]);
        assert_eq!(neoforge_prefix("1.21"), "21.0.");
        assert_eq!(neoforge_prefix("26.1"), "26.1.");
    }

    #[test]
    #[ignore = "Necesita red"]
    fn live_sources_return_recent_releases_and_loaders() {
        let root = std::env::temp_dir().join(format!("openclient-versions-live-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let value = catalog(&root, true).unwrap();
        let releases: Vec<&str> = value["versions"].as_array().unwrap().iter()
            .filter(|v| v["type"] == "release").filter_map(|v| v["id"].as_str()).collect();
        println!("releases={} newest={:?}", releases.len(), &releases[..6.min(releases.len())]);
        for key in ["fabric", "quilt", "forge", "neoforge"] {
            println!("{key}={:?}", value[key].as_array().map(|list| list.len()));
        }
        assert!(releases.contains(&"1.21.4") && releases.contains(&"1.21.5"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fresh_cache_is_used_without_network() {
        let root = std::env::temp_dir().join(format!("openclient-versions-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("cache")).unwrap();
        let cached = json!({"versions": [{"id": "1.21.11", "type": "release", "released": "2025-12-09"}],
                            "fabric": null, "fetchedAt": crate::session_time()});
        std::fs::write(root.join("cache/minecraft-versions.json"), cached.to_string()).unwrap();
        assert_eq!(catalog(&root, false).unwrap(), cached);
        std::fs::remove_dir_all(root).unwrap();
    }
}
