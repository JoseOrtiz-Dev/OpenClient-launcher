use crate::game::selection;
use openclient_core::library_state::{load_state_at, write_state_locked};
use serde_json::{json, Value};
use std::time::Duration;

#[cfg(test)]
mod project_tests;

const API: &str = "https://api.modrinth.com/v2";
const CURSEFORGE_PENDING: &str = "CurseForge estará disponible próximamente.";
/// Cargadores que aceptan los modpacks del catálogo.
const PACK_LOADERS: [&str; 4] = ["fabric", "quilt", "forge", "neoforge"];
const CONTENT_LIMIT: usize = 256;
const MAX_OPTIONAL: usize = 16;
const MAX_ANNOTATIONS: usize = 20;
const MAX_CONFLICTS: usize = 50;
const CACHE_LIMIT: u64 = 8 * 1024 * 1024;
const CONFLICT_MESSAGE: &str = "Esta versión declara una incompatibilidad con contenido activo de la instancia. Desactívalo o elige otra versión.";

fn reject_curseforge(request: &Value) -> Result<(), String> {
    let legacy_project = request["project"]
        .as_str()
        .and_then(|id| id.strip_prefix("cf"))
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|byte| byte.is_ascii_digit()));
    if request["provider"] == "curseforge" || legacy_project {
        return Err(CURSEFORGE_PENDING.into());
    }
    Ok(())
}

pub fn project(request: &Value) -> Result<Value, String> {
    reject_curseforge(request)?;
    let id = valid_id(&request["project"]).ok_or("Proyecto inválido.")?;
    let kind = request["kind"]
        .as_str()
        .ok_or("Falta el tipo de contenido.")?;
    let target = if kind == "modpack" {
        None
    } else {
        let (root, instance) = selection(request)?;
        content_kind(&request["kind"], &instance)?;
        Some((root, instance))
    };
    let game = match &target {
        Some((_, instance)) => Some(minecraft_version(instance)?.to_owned()),
        None => None,
    };
    run(async {
        let http = client()?;
        let metadata = response(http.get(format!("{API}/project/{id}"))).await?;
        let mut params = vec![("include_changelog", "false".to_owned())];
        if let Some(game) = &game {
            params.push(("game_versions", json!([game]).to_string()));
        }
        let loaders = project_loaders(kind);
        if !loaders.is_empty() {
            params.push(("loaders", json!(loaders).to_string()));
        }
        let versions = response(
            http.get(format!("{API}/project/{id}/version"))
                .query(&params),
        )
        .await?;
        let mut summary = project_summary(id, kind, game.as_deref(), &metadata, &versions)?;
        let mut ids: std::collections::BTreeSet<String> = summary["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|version| version["dependencies"].as_array().into_iter().flatten())
            .filter_map(|dependency| valid_id(&dependency["project_id"]))
            .map(str::to_owned)
            .collect();
        let version_ids: std::collections::BTreeSet<&str> = summary["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|version| version["dependencies"].as_array().into_iter().flatten())
            .filter_map(|dependency| valid_id(&dependency["version_id"]))
            .collect();
        let version_ids: Vec<&str> = version_ids.into_iter().take(100).collect();
        let mut dependency_versions = serde_json::Map::new();
        if !version_ids.is_empty() {
            if let Ok(versions) = response(
                http.get(format!("{API}/versions"))
                    .query(&[("ids", json!(version_ids).to_string())]),
            )
            .await
            {
                for version in versions.as_array().into_iter().flatten() {
                    if let Some(version_id) =
                        valid_id(&version["id"]).filter(|id| version_ids.contains(id))
                    {
                        if let Some(project_id) = valid_id(&version["project_id"]) {
                            ids.insert(project_id.to_owned());
                            dependency_versions.insert(version_id.into(), json!({"project":project_id,
                                "number":version["version_number"].as_str().unwrap_or("").chars().take(150).collect::<String>()}));
                        }
                    }
                }
            }
        }
        let ids: Vec<String> = ids.into_iter().take(100).collect();
        let mut names = serde_json::Map::new();
        if !ids.is_empty() {
            if let Ok(projects) = response(
                http.get(format!("{API}/projects"))
                    .query(&[("ids", json!(ids).to_string())]),
            )
            .await
            {
                for project in projects.as_array().into_iter().flatten() {
                    if let Some(id) =
                        valid_id(&project["id"]).filter(|id| ids.iter().any(|wanted| wanted == id))
                    {
                        if let Some(title) = project["title"].as_str() {
                            names.insert(
                                id.into(),
                                json!(title.chars().take(150).collect::<String>()),
                            );
                        }
                    }
                }
            }
        }
        summary["dependencyNames"] = Value::Object(names);
        summary["dependencyVersions"] = Value::Object(dependency_versions);
        match &target {
            Some((root, instance)) => {
                let mut cache = instance["id"]
                    .as_str()
                    .and_then(|id| read_cache(root, id))
                    .unwrap_or_else(|| json!({}));
                complete_cache(&http, &mut cache, &instance["installedContent"]).await;
                Ok(annotate_versions(
                    summary,
                    Some(&instance["installedContent"]),
                    &cache,
                ))
            }
            None => Ok(annotate_versions(summary, None, &Value::Null)),
        }
    })
}

fn project_loaders(kind: &str) -> Vec<&'static str> {
    if kind == "modpack" {
        PACK_LOADERS.to_vec()
    } else {
        modrinth_loader(kind).into_iter().collect()
    }
}

fn project_summary(
    id: &str,
    kind: &str,
    game: Option<&str>,
    metadata: &Value,
    versions: &Value,
) -> Result<Value, String> {
    if metadata["id"] != id || metadata["project_type"] != kind {
        return Err("El proyecto no corresponde al contenido solicitado.".into());
    }
    let versions: Vec<Value> = versions.as_array().ok_or("Modrinth devolvió versiones inválidas.")?
        .iter().filter(|item| version_compatible(item, id, kind, game))
        .map(|item| json!({"id":item["id"], "name":item["name"],
            "number":item["version_number"], "channel":item["version_type"],
            "dependencies":item["dependencies"].as_array().cloned().unwrap_or_default(),
            "date":item["date_published"], "games":item["game_versions"], "loaders":item["loaders"]})).collect();
    let recommended = versions
        .iter()
        .find(|item| item["channel"] == "release")
        .or_else(|| versions.first());
    let gallery: Vec<Value> = metadata["gallery"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            modrinth_icon(&item["url"]).map(|url| json!({"url":url, "title":item["title"]}))
        })
        .take(12)
        .collect();
    Ok(
        json!({"id":id, "kind":kind, "title":metadata["title"], "description":metadata["description"],
        "body":metadata["body"].as_str().unwrap_or("").chars().take(60000).collect::<String>(),
        "icon":modrinth_icon(&metadata["icon_url"]), "downloads":metadata["downloads"],
        "license":metadata["license"]["id"], "gallery":gallery,
        "versionCount":versions.len(), "recommended":recommended,
        "versions":versions.iter().take(100).collect::<Vec<_>>()}),
    )
}

fn version_compatible(item: &Value, project: &str, kind: &str, game: Option<&str>) -> bool {
    let loaders = project_loaders(kind);
    item["project_id"] == project
        && valid_id(&item["id"]).is_some()
        && (loaders.is_empty()
            || item["loaders"].as_array().is_some_and(|values| {
                loaders.iter().any(|loader| values.contains(&json!(loader)))
            }))
        && game.is_none_or(|game| {
            item["game_versions"]
                .as_array()
                .is_some_and(|values| values.contains(&json!(game)))
        })
        && item["files"].as_array().is_some_and(|files| {
            !files.is_empty()
                && (kind != "modpack"
                    || files.iter().any(|file| {
                        file["filename"]
                            .as_str()
                            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".mrpack"))
                    }))
        })
}

fn pick_version<'a>(
    versions: &'a [Value],
    project: &str,
    kind: &str,
    game: Option<&str>,
    requested: &Value,
) -> Result<&'a Value, String> {
    let compatible = |item: &&Value| version_compatible(item, project, kind, game);
    if !requested.is_null() {
        let id = valid_id(requested).ok_or("Identificador de versión inválido.")?;
        return versions
            .iter()
            .filter(compatible)
            .find(|item| item["id"] == id)
            .ok_or(
                "La versión elegida ya no está disponible o no es compatible con este destino."
                    .into(),
            );
    }
    versions
        .iter()
        .filter(compatible)
        .find(|item| item["version_type"] == "release")
        .or_else(|| versions.iter().find(compatible))
        .ok_or("No hay versiones compatibles con este destino.".into())
}

/// ¿Declara `dependency` una incompatibilidad con este proyecto o versión?
fn declares(dependency: &Value, project: &str, version: Option<&str>) -> bool {
    dependency["dependency_type"] == "incompatible"
        && match valid_id(&dependency["version_id"]) {
            Some(pinned) => version == Some(pinned),
            None => valid_id(&dependency["project_id"]) == Some(project),
        }
}

fn dependency_conflict(dependency: &Value, installed: &Value) -> bool {
    installed.as_array().into_iter().flatten().any(|item| {
        item["enabled"] == true
            && item["id"]
                .as_str()
                .is_some_and(|project| declares(dependency, project, item["versionId"].as_str()))
    })
}

fn dependencies_of(version: &Value) -> impl Iterator<Item = &Value> {
    version["dependencies"].as_array().into_iter().flatten()
}

fn short_title(item: &Value) -> String {
    item["title"]
        .as_str()
        .filter(|title| !title.is_empty())
        .or_else(|| item["id"].as_str())
        .unwrap_or_default()
        .chars()
        .take(150)
        .collect()
}

/// Versión de `item` en la caché de `sync_mods` (`version:<id>` o el id del proyecto).
pub(crate) fn cached_version<'a>(cache: &'a Value, item: &Value) -> Option<&'a Value> {
    let version = item["versionId"].as_str()?;
    cache.get(format!("version:{version}")).or_else(|| {
        let project = item["id"].as_str()?;
        cache.get(project).filter(|saved| saved["id"] == version)
    })
}

fn plain(path: &std::path::Path, directory: bool) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| {
        !metadata.file_type().is_symlink()
            && if directory {
                metadata.is_dir()
            } else {
                metadata.is_file() && metadata.len() <= CACHE_LIMIT
            }
    })
}

/// Lee, sin red, la caché de versiones que escribe `sync_mods` (`None` si falta o no es fiable).
pub(crate) fn read_cache(root: &std::path::Path, id: &str) -> Option<Value> {
    uuid::Uuid::parse_str(id).ok()?;
    let instances = root.join("instances");
    let game = instances.join(id);
    let mods = game.join("mods");
    if ![&instances, &game, &mods]
        .into_iter()
        .all(|path| plain(path, true))
    {
        return None;
    }
    let legacy = mods.join(".alba_versions.json");
    let path = if legacy.exists() {
        legacy
    } else {
        mods.join(".openclient_versions.json")
    };
    if !plain(&path, false) {
        return None;
    }
    serde_json::from_str::<Value>(&std::fs::read_to_string(path).ok()?)
        .ok()
        .filter(Value::is_object)
}

/// Último recurso: pide a Modrinth las versiones instaladas que la caché no tiene (error ignorado).
async fn complete_cache(http: &reqwest::Client, cache: &mut Value, installed: &Value) {
    let wanted: std::collections::BTreeSet<String> = installed
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| {
            item["enabled"] == true
                && item["source"].as_str().unwrap_or("modrinth") == "modrinth"
                && cached_version(cache, item).is_none()
        })
        .filter_map(|item| valid_id(&item["versionId"]).map(str::to_owned))
        .take(100)
        .collect();
    if wanted.is_empty() {
        return;
    }
    let ids: Vec<&String> = wanted.iter().collect();
    let Ok(versions) = response(
        http.get(format!("{API}/versions"))
            .query(&[("ids", json!(ids).to_string())]),
    )
    .await
    else {
        return;
    };
    if !cache.is_object() {
        *cache = json!({});
    }
    for version in versions.as_array().into_iter().flatten() {
        if let Some(id) = valid_id(&version["id"]).filter(|id| wanted.contains(*id)) {
            cache[format!("version:{id}")] = version.clone();
        }
    }
}

/// Conflictos de añadir `version` de `project` con el contenido activo: los que declara la
/// propia versión (`declared`) y los que declara contenido instalado contra ella (`reverse`).
fn conflicts_for(project: &str, version: &Value, installed: &Value, cache: &Value) -> Vec<Value> {
    let active = || {
        installed
            .as_array()
            .into_iter()
            .flatten()
            .filter(|item| item["enabled"] == true)
    };
    let mut found: Vec<Value> = Vec::new();
    let mut note = |item: &Value, reason: &str| {
        if found.len() < MAX_CONFLICTS
            && !found
                .iter()
                .any(|known| known["project"] == item["id"] && known["reason"] == reason)
        {
            found.push(json!({"project": item["id"], "title": short_title(item), "reason": reason}));
        }
    };
    for dependency in dependencies_of(version) {
        for item in active() {
            if item["id"]
                .as_str()
                .is_some_and(|other| declares(dependency, other, item["versionId"].as_str()))
            {
                note(item, "declared");
            }
        }
    }
    let number = version["id"].as_str();
    for item in active().filter(|item| item["id"] != project) {
        if let Some(saved) = cached_version(cache, item) {
            if dependencies_of(saved).any(|dependency| declares(dependency, project, number)) {
                note(item, "reverse");
            }
        }
    }
    found
}

fn dependency_list(version: &Value, kind: &str, names: &Value, pins: &Value) -> Vec<Value> {
    dependencies_of(version)
        .filter(|dependency| dependency["dependency_type"] == kind)
        .filter_map(|dependency| {
            let pinned = valid_id(&dependency["version_id"]);
            let project = valid_id(&dependency["project_id"])
                .or_else(|| pinned.and_then(|id| valid_id(&pins[id]["project"])))?;
            let title = names[project].as_str().unwrap_or(project);
            Some(json!({"project": project, "version": pinned, "title": title}))
        })
        .take(MAX_ANNOTATIONS)
        .collect()
}

/// Añade a cada versión `optional`, `incompatible` y, si hay instancia, `conflicts`.
fn annotate_versions(mut summary: Value, installed: Option<&Value>, cache: &Value) -> Value {
    let (names, pins) = (summary["dependencyNames"].clone(), summary["dependencyVersions"].clone());
    let project = summary["id"].as_str().unwrap_or_default().to_owned();
    let annotate = |version: &mut Value| {
        version["optional"] = json!(dependency_list(version, "optional", &names, &pins));
        version["incompatible"] = json!(dependency_list(version, "incompatible", &names, &pins));
        if let Some(installed) = installed {
            version["conflicts"] = json!(conflicts_for(&project, version, installed, cache));
        }
    };
    if let Some(versions) = summary["versions"].as_array_mut() {
        versions.iter_mut().for_each(&annotate);
    }
    if summary["recommended"].is_object() {
        annotate(&mut summary["recommended"]);
    }
    summary
}

/// Valida `optional[]` de `content_add` sin tocar la red.
fn validate_optional(request: &Value, content_len: usize) -> Result<Vec<&str>, String> {
    let list = match &request["optional"] {
        Value::Null => Vec::new(),
        Value::Array(list) => list.iter().collect(),
        _ => return Err("Los complementos opcionales no son válidos.".into()),
    };
    if list.len() > MAX_OPTIONAL {
        return Err(format!("Se admiten hasta {MAX_OPTIONAL} complementos opcionales."));
    }
    let mut seen = std::collections::HashSet::new();
    let mut optional = Vec::with_capacity(list.len());
    for value in list {
        let id = valid_id(value).ok_or("Complemento opcional inválido.")?;
        if id == request["project"] {
            return Err("Un proyecto no puede ser su propio complemento.".into());
        }
        if !seen.insert(id) {
            return Err("Complemento opcional duplicado.".into());
        }
        optional.push(id);
    }
    if content_len + 1 + optional.len() > CONTENT_LIMIT {
        return Err("Esta instancia ya tiene 256 proyectos añadidos.".into());
    }
    Ok(optional)
}

/// Pares de contenido activo incompatibles según la caché (sin red): `(primeros 50, total)`.
fn conflict_pairs(installed: &Value, cache: &Value) -> (Vec<Value>, usize) {
    let active: Vec<&Value> = installed
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item["enabled"] == true && valid_id(&item["id"]).is_some())
        .collect();
    let declares_against = |from: &Value, to: &Value| {
        cached_version(cache, from).is_some_and(|saved| {
            dependencies_of(saved).any(|dependency| {
                to["id"]
                    .as_str()
                    .is_some_and(|project| declares(dependency, project, to["versionId"].as_str()))
            })
        })
    };
    let (mut listed, mut count) = (Vec::new(), 0);
    for (index, a) in active.iter().enumerate() {
        for b in &active[index + 1..] {
            if a["id"] != b["id"] && (declares_against(a, b) || declares_against(b, a)) {
                count += 1;
                if listed.len() < MAX_CONFLICTS {
                    listed.push(json!({"a": a["id"], "aTitle": short_title(a),
                        "b": b["id"], "bTitle": short_title(b), "reason": "incompatible"}));
                }
            }
        }
    }
    (listed, count)
}

/// `content_conflicts` `{root,id}`: solo lectura y sin red.
pub fn content_conflicts(request: &Value) -> Result<Value, String> {
    let (root, instance) = selection(request)?;
    let Some(cache) = instance["id"].as_str().and_then(|id| read_cache(root, id)) else {
        return Ok(json!({"conflicts": [], "count": 0, "cacheMissing": true}));
    };
    let (conflicts, count) = conflict_pairs(&instance["installedContent"], &cache);
    Ok(json!({"conflicts": conflicts, "count": count}))
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(15))
        .user_agent("OpenClient/0.1 (Minecraft desktop launcher)")
        .build()
        .map_err(|_| "No se pudo iniciar la conexión con Modrinth.".into())
}

pub(crate) fn run<T>(future: impl std::future::Future<Output = Result<T, String>>) -> Result<T, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "No se pudo iniciar la consulta.".to_owned())?
        .block_on(future)
}

async fn response(request: reqwest::RequestBuilder) -> Result<Value, String> {
    request
        .send()
        .await
        .map_err(|_| "No se pudo conectar con Modrinth.".to_owned())?
        .error_for_status()
        .map_err(|error| {
            format!(
                "Modrinth no pudo completar la consulta: {}.",
                error
                    .status()
                    .map(|status| status.as_u16())
                    .unwrap_or_default()
            )
        })?
        .json()
        .await
        .map_err(|_| "Modrinth devolvió una respuesta inválida.".into())
}

pub fn search(request: &Value) -> Result<Value, String> {
    reject_curseforge(request)?;
    let query = request["query"].as_str().unwrap_or_default().trim();
    if query.chars().count() > 100 {
        return Err("La búsqueda admite hasta 100 caracteres.".into());
    }
    let mut facets = if request["kind"] == "modpack" {
        vec![
            vec!["project_type:modpack".into()],
            vec!["categories:fabric".into()],
        ]
    } else {
        let (_, instance) = selection(request)?;
        let kind = content_kind(&request["kind"], &instance)?;
        let version = minecraft_version(&instance)?;
        let mut facets = vec![
            vec![format!("project_type:{kind}")],
            vec![format!("versions:{version}")],
        ];
        if let Some(loader) = instance_loader(kind, &instance) {
            facets.push(vec![format!("categories:{loader}")]);
        }
        facets
    };
    let category = request["category"].as_str().unwrap_or_default();
    if !category.is_empty() {
        if category.len() > 36
            || !category
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'-')
        {
            return Err("Categoría inválida.".into());
        }
        facets.push(vec![format!("categories:{category}")]);
    }
    let order = match request["sort"].as_str().unwrap_or_default() {
        "" if query.is_empty() => "downloads",
        "" | "relevance" => "relevance",
        order @ ("downloads" | "newest" | "updated") => order,
        _ => return Err("Orden de búsqueda inválido.".into()),
    };
    let page = request["page"].as_u64().unwrap_or(0);
    if page > 100 {
        return Err("Página fuera de rango.".into());
    }
    let offset = (page * 24).to_string();
    let facets = json!(facets).to_string();
    run(async {
        let results = response(client()?.get(format!("{API}/search")).query(&[
            ("query", query),
            ("facets", &facets),
            ("limit", "24"),
            ("offset", &offset),
            ("index", order),
        ]))
        .await?;
        let total_hits = results["total_hits"].as_u64().unwrap_or(0);
        let hits = results["hits"]
            .as_array()
            .ok_or("El catálogo devolvió resultados inválidos.")?;
        let projects: Vec<Value> = hits
            .iter()
            .filter(|hit| valid_id(&hit["project_id"]).is_some())
            .map(|hit| {
                json!({
                    "id": hit["project_id"], "title": hit["title"], "author": hit["author"],
                    "description": hit["description"], "downloads": hit["downloads"],
                    "icon": modrinth_icon(&hit["icon_url"]),
                    "categories": hit["display_categories"],
                    "provider": "modrinth",
                })
            })
            .collect();
        if request["action"] == "catalog_discover" {
            Ok(json!({"projects": projects, "total": total_hits, "page": page}))
        } else {
            Ok(json!(projects))
        }
    })
}

fn modrinth_icon(value: &Value) -> Option<&str> {
    let icon = value.as_str()?;
    let url = reqwest::Url::parse(icon).ok()?;
    (url.scheme() == "https"
        && url.host_str() == Some("cdn.modrinth.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path().starts_with("/data/"))
    .then_some(icon)
}

pub fn inspect_pack(request: &Value) -> Result<Value, String> {
    let file = request["file"]
        .as_str()
        .ok_or("Selecciona un archivo .mrpack.")?;
    openclient_core::modpack::Modpack::open_local(std::path::Path::new(file))?.summary()
}

pub fn install_pack(request: &Value) -> Result<Value, String> {
    use openclient_core::modpack;
    use std::path::Path;

    let local = request["action"] == "modpack_import";
    if !local {
        reject_curseforge(request)?;
    }
    let root = Path::new(
        request["root"]
            .as_str()
            .ok_or("Falta la carpeta de datos.")?,
    );
    if !root.is_absolute() || !root.is_dir() {
        return Err("Carpeta de datos inválida.".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let downloads = modpack::client()?;
    let (pack, title, release, catalog_ids) = if local {
        let file = request["file"]
            .as_str()
            .ok_or("Selecciona un archivo .mrpack.")?;
        let pack = modpack::Modpack::open_local(Path::new(file))?;
        let (title, release) = pack.identity()?;
        let (title, release) = (json!(title), json!(release));
        (pack, title, release, (Value::Null, Value::Null))
    } else {
        let project = valid_id(&request["project"]).ok_or("Proyecto inválido.")?;
        runtime.block_on(async {
            let http = client()?;
            let metadata = response(http.get(format!("{API}/project/{project}"))).await?;
            if metadata["project_type"] != "modpack" || metadata["id"] != project {
                return Err("El proyecto no es un modpack de Modrinth.".into());
            }
            let versions = response(
                http.get(format!("{API}/project/{project}/version"))
                    .query(&[
                        ("loaders", json!(PACK_LOADERS).to_string().as_str()),
                        ("include_changelog", "false"),
                    ]),
            )
            .await?;
            let versions = versions
                .as_array()
                .ok_or("El catálogo devolvió versiones inválidas.")?;
            let version = pick_version(versions, project, "modpack", None, &request["version"])?;
            let mut files = version["files"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|file| {
                    file["filename"]
                        .as_str()
                        .is_some_and(|name| name.to_ascii_lowercase().ends_with(".mrpack"))
                });
            let file = files
                .clone()
                .find(|file| file["primary"] == true)
                .or_else(|| files.next())
                .ok_or("No se encontró el archivo .mrpack.")?;
            let bytes = modpack::download(
                &downloads,
                &file["url"],
                &file["hashes"]["sha512"],
                &file["size"],
            )
            .await?;
            let pack = modpack::Modpack::open(bytes)?;
            let dependencies = pack.pack_dependencies()?;
            if dependencies.loader == modpack::Loader::Vanilla
                || !version["game_versions"].as_array().is_some_and(|versions| {
                    versions.iter().any(|value| value == &dependencies.minecraft)
                })
            {
                return Err(
                    "El manifiesto del modpack no coincide con la versión del catálogo.".into(),
                );
            }
            Ok::<_, String>((
                pack,
                metadata["title"].clone(),
                version["version_number"].clone(),
                (json!(project), version["id"].clone()),
            ))
        })?
    };
    let dependencies = pack.pack_dependencies()?;
    let name: String = title
        .as_str()
        .unwrap_or("Modpack de Modrinth")
        .chars()
        .filter(|character| !character.is_control())
        .take(64)
        .collect();
    let name = name.trim();
    let official = request["official"].as_str().filter(|id| {
        local
            && !id.is_empty()
            && id.len() <= 32
            && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    });
    let source = match (local, official) {
        (false, _) => "modrinth",
        (true, Some(_)) => "official",
        (true, None) => "file",
    };
    let version_number: String = release
        .as_str()
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .take(100)
        .collect();
    let loader = dependencies.loader;
    let id = uuid::Uuid::new_v4();
    let mut entry = super::portable_metadata(&json!({
        "name": name, "mcVersion": dependencies.minecraft, "category": loader.category(),
        "fabricVersion": if loader == modpack::Loader::Fabric { json!(dependencies.loader_version) } else { Value::Null },
        "installedContent": [],
        "modpack": {
            "source": source, "projectId": catalog_ids.0, "versionId": catalog_ids.1,
            "versionNumber": version_number, "name": name,
            "loader": loader.category(), "loaderVersion": dependencies.loader_version
        }
    }))?;
    entry["id"] = json!(id.to_string());
    if let Some(official) = official {
        entry["official"] = json!(official);
    }
    let path = root.join("launcher_state.json");
    let mut state = match load_state_at(&path)? {
        Some(raw) => serde_json::from_str::<Value>(&raw).map_err(|_| "Biblioteca inválida.")?,
        None => json!({"instances": []}),
    };
    state["instances"]
        .as_array_mut()
        .ok_or("Biblioteca inválida.")?
        .push(entry);
    let context = match crate::operation::current() {
        Some(context) => context,
        None => crate::operation::detached_context(request)?,
    };
    pack.publish_with(root, id, &state.to_string(), &runtime, &downloads, &context)?;
    if local {
        Ok(state["instances"].take())
    } else {
        Ok(json!({"id": id.to_string()}))
    }
}

fn content_kind<'a>(value: &'a Value, instance: &Value) -> Result<&'a str, String> {
    let loader = instance["category"].as_str().unwrap_or("vanilla");
    match value.as_str().unwrap_or("mod") {
        kind @ ("mod" | "shader")
            if matches!(loader, "fabric" | "quilt" | "forge" | "neoforge") =>
        {
            Ok(kind)
        }
        "resourcepack"
            if matches!(
                loader,
                "vanilla" | "fabric" | "quilt" | "forge" | "neoforge"
            ) =>
        {
            Ok("resourcepack")
        }
        "mod" | "shader" => {
            Err("Selecciona una instancia con cargador de mods (Fabric, Forge, NeoForge o Quilt) para este contenido.".into())
        }
        _ => Err("Tipo de contenido no compatible.".into()),
    }
}

fn modrinth_loader(kind: &str) -> Option<&'static str> {
    match kind {
        "mod" => Some("fabric"),
        "shader" => Some("iris"),
        _ => None,
    }
}

fn instance_loader<'a>(kind: &str, instance: &'a Value) -> Option<&'a str> {
    match kind {
        "mod" => match instance["category"].as_str() {
            Some(l @ ("fabric" | "quilt" | "forge" | "neoforge")) => Some(l),
            _ => Some("fabric"),
        },
        "shader" => Some("iris"),
        _ => None,
    }
}

fn minecraft_version(instance: &Value) -> Result<&str, String> {
    instance["mcVersion"]
        .as_str()
        .ok_or("Versión de Minecraft inválida.".into())
}

fn valid_id(value: &Value) -> Option<&str> {
    value.as_str().filter(|id| {
        !id.is_empty() && id.len() <= 32 && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
    })
}

pub fn change(request: &Value) -> Result<Value, String> {
    if request["action"] == "content_add" {
        reject_curseforge(request)?;
    }
    let (root, instance) = selection(request)?;
    let project = valid_id(&request["project"]).ok_or("Proyecto inválido.")?;
    let path = root.join("launcher_state.json");
    let raw = load_state_at(&path)?.ok_or("La biblioteca está vacía.")?;
    let mut state: Value = serde_json::from_str(&raw).map_err(|_| "Biblioteca inválida.")?;
    let entry = state["instances"]
        .as_array_mut()
        .and_then(|entries| {
            entries
                .iter_mut()
                .find(|entry| entry["id"] == instance["id"])
        })
        .ok_or("La instancia ya no existe.")?;
    let content = entry["installedContent"]
        .as_array_mut()
        .ok_or("Contenido inválido.")?;
    let position = content.iter().position(|item| item["id"] == project);
    let mut skipped_optional = None;
    match request["action"].as_str() {
        Some("content_add") => {
            let kind = content_kind(&request["kind"], &instance)?;
            let version = minecraft_version(&instance)?;
            if position.is_some() {
                return Err("Este proyecto ya está añadido.".into());
            }
            let optional = validate_optional(request, content.len())?;
            let (items, skipped) = run(async {
                let http = client()?;
                let installed = &instance["installedContent"];
                let mut cache = read_cache(root, instance["id"].as_str().unwrap_or_default())
                    .unwrap_or_else(|| json!({}));
                complete_cache(&http, &mut cache, installed).await;
                let metadata = response(http.get(format!("{API}/project/{project}"))).await?;
                let picked = resolve_version(
                    &http,
                    project,
                    kind,
                    instance_loader(kind, &instance),
                    version,
                    &request["version"],
                    &metadata,
                )
                .await?;
                if !conflicts_for(project, &picked, installed, &cache).is_empty() {
                    return Err(CONFLICT_MESSAGE.into());
                }
                let main = json!({"id": project, "versionId": picked["id"], "version": picked["version_number"],
                    "title": metadata["title"], "type": kind, "source": "modrinth", "enabled": true});
                let mut staged = installed.as_array().cloned().unwrap_or_default();
                staged.push(main.clone());
                let mut items = vec![main];
                let mut skipped = Vec::new();
                if !optional.is_empty() {
                    let mut skip = |id: &str, reason: &str| {
                        skipped.push(json!({"project": id, "reason": reason.chars().take(150).collect::<String>()}));
                    };
                    let loader = content_kind(&json!("mod"), &instance)
                        .map(|kind| instance_loader(kind, &instance));
                    match (loader, response(
                        http.get(format!("{API}/projects"))
                            .query(&[("ids", json!(optional).to_string())]),
                    )
                    .await)
                    {
                        (Err(reason), _) | (_, Err(reason)) => {
                            optional.iter().for_each(|id| skip(id, &reason));
                        }
                        (Ok(loader), Ok(projects)) => {
                            for id in &optional {
                                let Some(found) = projects
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .find(|item| item["id"] == *id)
                                else {
                                    skip(id, "El proyecto ya no existe.");
                                    continue;
                                };
                                if found["project_type"] != "mod" {
                                    skip(id, "El proyecto no es un mod.");
                                    continue;
                                }
                                if staged.iter().any(|item| item["id"] == *id) {
                                    skip(id, "Ya está añadido.");
                                    continue;
                                }
                                let picked = match resolve_version(&http, id, "mod", loader, version, &Value::Null, found).await {
                                    Ok(picked) => picked,
                                    Err(reason) => {
                                        skip(id, &reason);
                                        continue;
                                    }
                                };
                                if !conflicts_for(id, &picked, &json!(staged), &cache).is_empty() {
                                    skip(id, "Declara una incompatibilidad con contenido activo de la instancia.");
                                    continue;
                                }
                                let item = json!({"id": id, "versionId": picked["id"], "version": picked["version_number"],
                                    "title": found["title"], "type": "mod", "source": "modrinth", "enabled": true,
                                    "addedAs": "optional", "parent": project});
                                staged.push(item.clone());
                                items.push(item);
                            }
                        }
                    }
                }
                Ok::<_, String>((items, skipped))
            })?;
            content.extend(items);
            skipped_optional = (!optional.is_empty()).then_some(skipped);
        }
        Some("content_toggle") => {
            let item = &mut content[position.ok_or("El proyecto ya no está en la instancia.")?];
            item["enabled"] = json!(!item["enabled"].as_bool().unwrap_or(false));
        }
        Some("content_remove") => {
            content.remove(position.ok_or("El proyecto ya no está en la instancia.")?);
        }
        _ => return Err("Operación de contenido inválida.".into()),
    }
    write_state_locked(&path, &state.to_string())?;
    let _ = crate::game::sync_content(request);
    match skipped_optional {
        Some(skipped) => Ok(json!({"instances": state["instances"].take(), "skippedOptional": skipped})),
        None => Ok(state["instances"].take()),
    }
}

/// Versión compatible de `project` para añadirla a una instancia (la elegida o la mejor).
async fn resolve_version(
    http: &reqwest::Client,
    project: &str,
    kind: &str,
    loader: Option<&str>,
    game: &str,
    requested: &Value,
    metadata: &Value,
) -> Result<Value, String> {
    if metadata["project_type"] != kind || metadata["id"] != project {
        return Err("El proyecto pertenece a otra categoría.".into());
    }
    let mut params = vec![
        ("game_versions", json!([game]).to_string()),
        ("include_changelog", "false".into()),
    ];
    if let Some(loader) = loader {
        params.push(("loaders", json!([loader]).to_string()));
    }
    let versions = response(
        http.get(format!("{API}/project/{project}/version"))
            .query(&params),
    )
    .await?;
    let choices = versions
        .as_array()
        .ok_or("No se pudieron leer las versiones.")?;
    let compatible = |item: &&Value| {
        item["project_id"] == project
            && valid_id(&item["id"]).is_some()
            && loader.is_none_or(|l| {
                item["loaders"]
                    .as_array()
                    .is_some_and(|values| values.contains(&json!(l)))
            })
            && item["game_versions"]
                .as_array()
                .is_some_and(|values| values.contains(&json!(game)))
            && item["files"]
                .as_array()
                .is_some_and(|files| !files.is_empty())
    };
    let picked = if !requested.is_null() {
        let wanted = valid_id(requested).ok_or("Identificador de versión inválido.")?;
        choices
            .iter()
            .filter(compatible)
            .find(|item| item["id"] == wanted)
            .ok_or_else(|| "La versión elegida ya no está disponible o no es compatible con este destino.".to_owned())?
    } else {
        choices
            .iter()
            .filter(compatible)
            .find(|item| item["version_type"] == "release")
            .or_else(|| choices.iter().find(compatible))
            .ok_or_else(|| "No hay versiones compatibles con este destino.".to_owned())?
    };
    Ok(picked.clone())
}

pub fn update_content(request: &Value) -> Result<Value, String> {
    let (root, instance) = selection(request)?;
    let mc = minecraft_version(&instance)?.to_owned();
    let path = root.join("launcher_state.json");
    let raw = load_state_at(&path)?.ok_or("La biblioteca está vacía.")?;
    let mut state: Value = serde_json::from_str(&raw).map_err(|_| "Biblioteca inválida.")?;
    let entry = state["instances"]
        .as_array_mut()
        .and_then(|entries| entries.iter_mut().find(|entry| entry["id"] == instance["id"]))
        .ok_or("La instancia ya no existe.")?;
    let snapshot = entry["installedContent"].clone();
    let content = entry["installedContent"]
        .as_array_mut()
        .ok_or("Contenido inválido.")?;
    let mut updated = 0u64;
    if !content.is_empty() {
        run(async {
            let http = client()?;
            for item in content.iter_mut() {
                if item["source"].as_str().unwrap_or("modrinth") != "modrinth" {
                    continue;
                }
                let Some(project) = valid_id(&item["id"]).map(str::to_owned) else {
                    continue;
                };
                let kind = item["type"].as_str().unwrap_or("mod").to_owned();
                let mut params = vec![
                    ("game_versions", json!([&mc]).to_string()),
                    ("include_changelog", "false".into()),
                ];
                if let Some(loader) = modrinth_loader(&kind) {
                    params.push(("loaders", json!([loader]).to_string()));
                }
                let Ok(versions) = response(
                    http.get(format!("{API}/project/{project}/version"))
                        .query(&params),
                )
                .await
                else {
                    continue;
                };
                let Some(choices) = versions.as_array() else {
                    continue;
                };
                let Ok(picked) = pick_version(choices, &project, &kind, Some(&mc), &Value::Null)
                else {
                    continue;
                };
                if picked["dependencies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|dep| dependency_conflict(dep, &snapshot))
                {
                    continue;
                }
                if item["versionId"] != picked["id"] {
                    item["versionId"] = picked["id"].clone();
                    item["version"] = picked["version_number"].clone();
                    updated += 1;
                }
            }
            Ok::<(), String>(())
        })?;
        if updated > 0 {
            write_state_locked(&path, &state.to_string())?;
        }
    }
    let _ = crate::game::sync_content(request);
    Ok(json!({"instances": state["instances"].take(), "updated": updated}))
}
