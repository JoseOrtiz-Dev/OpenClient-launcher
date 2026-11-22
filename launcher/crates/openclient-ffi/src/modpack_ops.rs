//! Acciones de modpack vinculado: comprobar, vincular, actualizar, copias de seguridad
//! y restauración. Toda la entrada se valida antes de tocar la red o el disco; los cambios
//! en la instancia los hace `modpack_tx` (copia de seguridad, rollback y `recover`).

use crate::operation;
use openclient_core::{
    cancel::guard,
    library_state::{load_state_at, write_state_locked},
    modpack::{
        self, fabric_version_valid, read_manifest, write_manifest, Loader, Modpack,
        PackDependencies, PackFile, PackManifest,
    },
    modpack_tx, GameContext,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

const API: &str = "https://api.modrinth.com/v2";
const UNLINKED: &str = "Vincula el modpack antes de actualizarlo.";
const LOADERS: [&str; 4] = ["fabric", "quilt", "forge", "neoforge"];
/// Vida de una consulta de versiones y antigüedad mínima para que `force` la ignore.
const CHECK_TTL: Duration = Duration::from_secs(10 * 60);
const FORCE_AFTER: Duration = Duration::from_secs(30);
/// Máximo de versiones y de rutas conservadas que se devuelven a la interfaz.
const LISTED: usize = 50;
/// Máximo de versiones que se guardan en la caché (la respuesta muestra `LISTED`).
const CACHED: usize = 300;
const MANAGED_LIMIT: u64 = 1024 * 1024;

type Checks = HashMap<String, (Instant, Value)>;

/// Versiones por proyecto de Modrinth. La DLL permanece cargada, así que sobrevive entre llamadas.
static CHECKS: LazyLock<Mutex<Checks>> = LazyLock::new(Mutex::default);

// ------------------------------------------------------------------ validación

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 32 && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// Id de instancia: un UUID en su forma canónica (también es un componente de ruta seguro).
fn instance_id(request: &Value) -> Result<&str, String> {
    let id = request["id"].as_str().ok_or("Falta el identificador.")?;
    if !uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id) {
        return Err("Identificador de instancia inválido.".into());
    }
    Ok(id)
}

/// `^[0-9]{1,20}-[0-9a-f]{8}$`
fn valid_snapshot_id(id: &str) -> bool {
    let Some((time, tag)) = id.split_once('-') else {
        return false;
    };
    (1..=20).contains(&time.len())
        && time.bytes().all(|byte| byte.is_ascii_digit())
        && tag.len() == 8
        && tag.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn clean(text: &str, max: usize) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Ruta de un `.mrpack` local: absoluta y con extensión `.mrpack` (el resto lo valida `open_local`).
fn local_pack_path(file: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(file);
    let is_pack = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mrpack"));
    if file.is_empty() || !path.is_absolute() || !is_pack {
        return Err("Selecciona un archivo .mrpack.".into());
    }
    Ok(path)
}

fn official_id(value: &Value) -> Result<Option<String>, String> {
    match value {
        Value::Null => Ok(None),
        Value::String(id)
            if !id.is_empty()
                && id.len() <= 32
                && id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') =>
        {
            Ok(Some(id.clone()))
        }
        _ => Err("Identificador del modpack oficial inválido.".into()),
    }
}

// ------------------------------------------------------------------ biblioteca

fn load_library(root: &Path) -> Result<Value, String> {
    let raw = load_state_at(&root.join("launcher_state.json"))?
        .ok_or("La biblioteca aún no existe.")?;
    serde_json::from_str(&raw).map_err(|_| "Biblioteca inválida.".to_owned())
}

fn find_entry<'a>(state: &'a Value, id: &str) -> Result<&'a Value, String> {
    state["instances"]
        .as_array()
        .and_then(|items| items.iter().find(|entry| entry["id"].as_str() == Some(id)))
        .ok_or_else(|| "La instancia ya no existe.".to_owned())
}

fn directory(path: &Path, missing: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| missing.to_owned())?;
    let canonical = path.canonicalize().map_err(|error| error.to_string())?;
    if !metadata.is_dir() || canonical != path {
        return Err("La carpeta de la instancia apunta a otra ubicación.".into());
    }
    Ok(())
}

/// `(instances, instances/<id>)` canónicos; la instancia debe existir y no ser un enlace.
fn locate(root: &Path, id: &str) -> Result<(PathBuf, PathBuf), String> {
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let instances = root.join("instances");
    directory(&instances, "La instancia no existe.")?;
    let game = instances.join(id);
    directory(&game, "La instancia no existe.")?;
    Ok((instances, game))
}

/// Carpeta temporal junto a las instancias (mismo volumen que el destino). Se borra al soltarla,
/// así que cancelar o fallar antes de `apply` no deja nada.
struct Staging {
    path: PathBuf,
}

impl Staging {
    fn new(instances: &Path) -> Result<Self, String> {
        let path = instances.join(format!(".openclient-stage-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path)
            .map_err(|error| format!("No se pudo preparar la actualización: {error}"))?;
        Ok(Self { path })
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Nombres de `mods/.openclient_managed.json` (contenido añadido por el usuario desde el catálogo).
fn read_managed(game: &Path) -> Result<HashSet<String>, String> {
    let path = game.join("mods").join(".openclient_managed.json");
    let damaged = || "La lista de contenido gestionado está dañada.".to_owned();
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashSet::new()),
        Err(error) => return Err(error.to_string()),
    };
    if !metadata.is_file() || metadata.len() > MANAGED_LIMIT {
        return Err(damaged());
    }
    let names: Vec<String> =
        serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
            .map_err(|_| damaged())?;
    Ok(names.into_iter().collect())
}

// ------------------------------------------------------------------- vínculo

fn loader_label(category: &str) -> &'static str {
    match category {
        "fabric" => "Fabric",
        "quilt" => "Quilt",
        "forge" => "Forge",
        "neoforge" => "NeoForge",
        _ => "Vanilla",
    }
}

/// Vínculo de la entrada de estado, revalidado con las reglas de `portable_metadata`.
fn build_link(
    source: &str,
    project: Option<&str>,
    version: Option<&str>,
    number: &str,
    name: &str,
    dependencies: &PackDependencies,
) -> Result<Value, String> {
    let link = json!({
        "source": source, "projectId": project, "versionId": version,
        "versionNumber": clean(number, 100), "name": clean(name, 64),
        "loader": dependencies.loader.category(), "loaderVersion": dependencies.loader_version
    });
    crate::portable_modpack(&link, &|value: &Value| value.as_str().is_some_and(valid_id))?
        .ok_or_else(|| "El vínculo con el modpack no es válido.".to_owned())
}

/// Aplica a la entrada lo que el pack manda: Minecraft, cargador y vínculo.
fn apply_pack(entry: &mut Value, dependencies: &PackDependencies, link: &Value) -> Result<(), String> {
    let category = dependencies.loader.category();
    entry["mcVersion"] = json!(dependencies.minecraft);
    entry["category"] = json!(category);
    entry["loaderLabel"] = json!(loader_label(category));
    match (&dependencies.loader, &dependencies.loader_version) {
        (Loader::Fabric, Some(version)) => {
            if !fabric_version_valid(version) {
                return Err("La versión de Fabric no es válida.".into());
            }
            entry["fabricVersion"] = json!(version);
        }
        _ => {
            if let Some(fields) = entry.as_object_mut() {
                fields.remove("fabricVersion");
            }
        }
    }
    entry["modpack"] = link.clone();
    entry["modpackVersion"] = link["versionNumber"].clone();
    Ok(())
}

fn replace_entry(state: &Value, id: &str, entry: &Value) -> Result<Value, String> {
    let mut next = state.clone();
    let slot = next["instances"]
        .as_array_mut()
        .and_then(|items| items.iter_mut().find(|item| item["id"].as_str() == Some(id)))
        .ok_or("La instancia ya no existe.")?;
    *slot = entry.clone();
    Ok(next)
}

// ----------------------------------------------------------------- Modrinth

fn api_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(15))
        .user_agent("OpenClient/0.1 (Minecraft desktop launcher)")
        .build()
        .map_err(|_| "No se pudo iniciar la conexión con Modrinth.".to_owned())
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "No se pudo iniciar la consulta.".to_owned())
}

async fn api_get(
    client: &reqwest::Client,
    url: &str,
    query: &[(&str, &str)],
) -> Result<Value, String> {
    client
        .get(url)
        .query(query)
        .send()
        .await
        .map_err(|_| "No se pudo conectar con Modrinth.".to_owned())?
        .error_for_status()
        .map_err(|error| {
            format!(
                "Modrinth no pudo completar la consulta: {}.",
                error.status().map(|status| status.as_u16()).unwrap_or_default()
            )
        })?
        .json()
        .await
        .map_err(|_| "Modrinth devolvió una respuesta inválida.".to_owned())
}

fn mrpack_file(version: &Value) -> Option<&Value> {
    let is_pack = |file: &&Value| {
        file["filename"]
            .as_str()
            .is_some_and(|name| name.to_ascii_lowercase().ends_with(".mrpack"))
    };
    let files = version["files"].as_array()?;
    files
        .iter()
        .filter(is_pack)
        .find(|file| file["primary"] == true)
        .or_else(|| files.iter().find(is_pack))
}

/// Versión de la lista de Modrinth reducida a lo que muestra la interfaz; `None` si no es un
/// `.mrpack` instalable o si declara un cargador que OpenClient no admite.
fn summarize(version: &Value) -> Option<Value> {
    let id = version["id"].as_str().filter(|id| valid_id(id))?;
    mrpack_file(version)?;
    let loaders = version["loaders"].as_array()?;
    if loaders.is_empty()
        || !loaders
            .iter()
            .all(|loader| loader.as_str().is_some_and(|loader| LOADERS.contains(&loader)))
    {
        return None;
    }
    let channel = version["version_type"]
        .as_str()
        .filter(|channel| matches!(*channel, "release" | "beta" | "alpha"))?;
    let minecraft: Vec<&str> = version["game_versions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|game| game.len() <= 40 && !game.chars().any(char::is_control))
        .take(20)
        .collect();
    Some(json!({
        "id": id, "number": clean(version["version_number"].as_str()?, 100), "channel": channel,
        "date": clean(version["date_published"].as_str().unwrap_or_default(), 40),
        "minecraft": minecraft, "loaders": loaders
    }))
}

/// Descarga el `.mrpack` de una versión de Modrinth (comprobando que pertenece al proyecto).
async fn fetch_modrinth_pack(
    project: &str,
    version: &str,
    ctx: &GameContext,
) -> Result<(Modpack, Value), String> {
    let api = api_client()?;
    let downloads = modpack::client()?;
    let metadata = guard(&ctx.cancel, api_get(&api, &format!("{API}/version/{version}"), &[])).await?;
    if metadata["id"] != version || metadata["project_id"] != project {
        return Err("La versión elegida no pertenece a este modpack.".into());
    }
    let file = mrpack_file(&metadata).ok_or("No se encontró el archivo .mrpack.")?;
    let bytes = modpack::download_cancellable(
        &downloads,
        &file["url"],
        &file["hashes"]["sha512"],
        &file["size"],
        &ctx.cancel,
    )
    .await?;
    Ok((Modpack::open(bytes)?, metadata))
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

// --------------------------------------------------------------- modpack_check

/// Consulta de versiones aún válida para `project`: dentro de 10 min y, con `force`, solo si la
/// última consulta tiene menos de 30 s. Función pura sobre el mapa (la usan las pruebas).
fn cached_in(checks: &Checks, project: &str, force: bool, now: Instant) -> Option<Value> {
    let (at, value) = checks.get(project)?;
    let age = now.saturating_duration_since(*at);
    (age < CHECK_TTL && (!force || age < FORCE_AFTER)).then(|| value.clone())
}

fn cached_check(project: &str, force: bool, now: Instant) -> Option<Value> {
    let checks = CHECKS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    cached_in(&checks, project, force, now)
}

fn store_check(project: &str, versions: Value, now: Instant) {
    let mut checks = CHECKS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    checks.retain(|_, (at, _)| now.saturating_duration_since(*at) < CHECK_TTL);
    if checks.len() >= 64 {
        checks.clear();
    }
    checks.insert(project.to_owned(), (now, versions));
}

/// Versiones de un proyecto de Modrinth ya resumidas, más recientes primero.
fn fetch_versions(project: &str) -> Result<Value, String> {
    runtime()?.block_on(async {
        let list = api_get(
            &api_client()?,
            &format!("{API}/project/{project}/version"),
            &[("include_changelog", "false")],
        )
        .await?;
        let list = list
            .as_array()
            .ok_or("Modrinth devolvió una respuesta inválida.")?;
        Ok::<_, String>(Value::Array(
            list.iter().filter_map(summarize).take(CACHED).collect(),
        ))
    })
}

fn check_response(link: &Value, versions: &[Value], manifest: bool) -> Value {
    let loader = link["loader"].as_str().unwrap_or_default();
    let latest = versions.iter().find(|version| {
        version["channel"] == "release"
            && version["loaders"]
                .as_array()
                .is_some_and(|loaders| loaders.iter().any(|item| item == loader))
    });
    json!({
        "linked": true, "source": link["source"],
        "current": {"versionId": link["versionId"], "versionNumber": link["versionNumber"]},
        "latest": latest,
        "updateAvailable": latest.is_some_and(|version| version["id"] != link["versionId"]),
        "versions": versions.iter().take(LISTED).collect::<Vec<_>>(),
        "legacy": false, "manifest": manifest
    })
}

/// `modpack_check` `{root,id,force}` → `{linked,source,current,latest,updateAvailable,versions,legacy,manifest}`.
/// Solo lectura. Las consultas a Modrinth se cachean 10 min por proyecto.
pub fn check(request: &Value) -> Result<Value, String> {
    let root = operation::root_of(request)?;
    let id = instance_id(request)?;
    let state = load_library(&root)?;
    let entry = find_entry(&state, id)?;
    let manifest = read_manifest(&root.join("instances").join(id)).is_ok_and(|found| found.is_some());
    let link = &entry["modpack"];
    if !link.is_object() {
        return Ok(json!({
            "linked": false, "updateAvailable": false, "versions": [],
            "legacy": entry["modpackVersion"].is_string(), "manifest": manifest
        }));
    }
    let project = link["projectId"].as_str().filter(|project| valid_id(project));
    let (true, Some(project)) = (link["source"] == "modrinth", project) else {
        return Ok(json!({
            "linked": true, "source": link["source"],
            "current": {"versionId": link["versionId"], "versionNumber": link["versionNumber"]},
            "latest": null, "updateAvailable": false, "versions": [],
            "legacy": false, "manifest": manifest
        }));
    };
    let force = request["force"].as_bool() == Some(true);
    let versions = match cached_check(project, force, Instant::now()) {
        Some(versions) => versions,
        None => {
            let versions = fetch_versions(project)?;
            store_check(project, versions.clone(), Instant::now());
            versions
        }
    };
    let listed: &[Value] = versions.as_array().map_or(&[], Vec::as_slice);
    Ok(check_response(link, listed, manifest))
}

// ------------------------------------------------------- modpack_link_candidates

/// `modpack_link_candidates` `{root,id}` → `[{"project","title","icon","versionId"|null}]` (≤ 5).
/// Solo lectura.
pub fn link_candidates(request: &Value) -> Result<Value, String> {
    let root = operation::root_of(request)?;
    let id = instance_id(request)?;
    let state = load_library(&root)?;
    let entry = find_entry(&state, id)?;
    let name = clean(entry["name"].as_str().unwrap_or_default(), 64);
    if name.is_empty() {
        return Err("La instancia no tiene nombre.".into());
    }
    let release = entry["modpackVersion"].as_str().map(str::to_owned);
    let mut facets = vec![json!(["project_type:modpack"])];
    if let Some(game) = entry["mcVersion"].as_str().filter(|game| {
        !game.is_empty()
            && game.len() <= 40
            && game.bytes().all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
    }) {
        facets.push(json!([format!("versions:{game}")]));
    }
    if let Some(loader) = entry["category"].as_str().filter(|loader| LOADERS.contains(loader)) {
        facets.push(json!([format!("categories:{loader}")]));
    }
    let facets = Value::Array(facets).to_string();
    runtime()?.block_on(async {
        let api = api_client()?;
        let results = api_get(
            &api,
            &format!("{API}/search"),
            &[("query", &name), ("facets", &facets), ("limit", "5"), ("index", "relevance")],
        )
        .await?;
        let hits = results["hits"]
            .as_array()
            .ok_or("El catálogo devolvió resultados inválidos.")?;
        let mut candidates = Vec::new();
        for hit in hits.iter().take(5) {
            let Some(project) = hit["project_id"].as_str().filter(|project| valid_id(project)) else {
                continue;
            };
            // Un fallo al pedir las versiones de un candidato no invalida la búsqueda.
            let version = match api_get(
                &api,
                &format!("{API}/project/{project}/version"),
                &[("include_changelog", "false")],
            )
            .await
            {
                Ok(list) => list.as_array().and_then(|versions| {
                    versions
                        .iter()
                        .filter(|version| mrpack_file(version).is_some())
                        .find(|version| version["version_number"].as_str() == release.as_deref())
                        .and_then(|version| version["id"].as_str().filter(|id| valid_id(id)))
                        .map(str::to_owned)
                }),
                Err(_) => None,
            };
            candidates.push(json!({
                "project": project, "title": clean(hit["title"].as_str().unwrap_or_default(), 100),
                "icon": modrinth_icon(&hit["icon_url"]), "versionId": version
            }));
        }
        Ok::<_, String>(Value::Array(candidates))
    })
}

// ------------------------------------------------------------------ modpack_link

enum LinkSource {
    Modrinth { project: String, version: String },
    File(PathBuf),
}

fn link_source(request: &Value) -> Result<LinkSource, String> {
    match (&request["project"], &request["version"], &request["file"]) {
        (Value::String(project), Value::String(version), Value::Null) => {
            if !valid_id(project) {
                return Err("Proyecto inválido.".into());
            }
            if !valid_id(version) {
                return Err("Identificador de versión inválido.".into());
            }
            Ok(LinkSource::Modrinth {
                project: project.clone(),
                version: version.clone(),
            })
        }
        (Value::Null, Value::Null, Value::String(file)) => Ok(LinkSource::File(local_pack_path(file)?)),
        _ => Err("Indica el modpack de Modrinth o un archivo .mrpack.".into()),
    }
}

/// `modpack_link` `{root,id,project,version}` o `{root,id,file}` →
/// `{"instances":[…],"adopted":N,"missing":M}`. Adopta solo los archivos del pack cuyo SHA-512
/// en disco coincide exactamente; el resto se considera del usuario. Operación registrada.
pub fn link(request: &Value, ctx: &GameContext) -> Result<Value, String> {
    let id = instance_id(request)?;
    let source = link_source(request)?;
    let root = ctx.root.clone();
    modpack_tx::recover(&root, id)?;
    let state = load_library(&root)?;
    let entry = find_entry(&state, id)?.clone();
    let (instances, game) = locate(&root, id)?;

    ctx.progress("downloading", 0, "Descargando el modpack");
    let (mut pack, kind, project, version, number, name) = match &source {
        LinkSource::Modrinth { project, version } => runtime()?.block_on(async {
            let metadata = guard(
                &ctx.cancel,
                api_get(&api_client()?, &format!("{API}/project/{project}"), &[]),
            )
            .await?;
            if metadata["project_type"] != "modpack" || metadata["id"] != project.as_str() {
                return Err("El proyecto no es un modpack de Modrinth.".to_owned());
            }
            let (pack, found) = fetch_modrinth_pack(project, version, ctx).await?;
            let number = found["version_number"].as_str().unwrap_or_default().to_owned();
            let title = metadata["title"].as_str().unwrap_or_default().to_owned();
            Ok::<_, String>((
                pack,
                "modrinth",
                Some(project.clone()),
                Some(version.clone()),
                number,
                title,
            ))
        })?,
        LinkSource::File(path) => {
            let pack = Modpack::open_local(path)?;
            let (name, number) = pack.identity()?;
            let (name, number) = (name.to_owned(), number.to_owned());
            let kind = if entry["official"].is_string() { "official" } else { "file" };
            (pack, kind, None, None, number, name)
        }
    };
    let dependencies = pack.pack_dependencies()?;
    if entry["mcVersion"] != json!(dependencies.minecraft)
        || entry["category"] != json!(dependencies.loader.category())
    {
        return Err(
            "El modpack no coincide con la versión de Minecraft ni con el cargador de la instancia."
                .into(),
        );
    }
    let link = build_link(kind, project.as_deref(), version.as_deref(), &number, &name, &dependencies)?;

    // Se prepara en staging solo para conocer los SHA-512 del pack; la instancia no se toca.
    let staging = Staging::new(&instances)?;
    let files = runtime()?.block_on(modpack_tx::stage(
        &mut pack,
        &staging.path,
        &modpack::client()?,
        ctx,
    ))?;
    ctx.cancel.check()?;
    let plan = modpack_tx::plan(&game, &PackManifest::default(), &pack, &files, &HashSet::new())?;
    let skipped: HashSet<String> = plan
        .add
        .iter()
        .chain(&plan.replace)
        .map(|path| path.to_lowercase())
        .collect();
    let adopted: Vec<PackFile> = plan
        .new_manifest
        .files
        .iter()
        .filter(|file| !skipped.contains(&file.path.to_lowercase()))
        .cloned()
        .collect();
    let missing = files.len().saturating_sub(adopted.len());

    let mut next = entry.clone();
    apply_pack(&mut next, &dependencies, &link)?;
    let after = replace_entry(&state, id, &next)?;
    ctx.cancel.check()?;
    ctx.progress("writing", 100, "Guardando el vínculo");
    let previous = read_manifest(&game).ok().flatten();
    let adopted_count = adopted.len();
    write_manifest(&game, &PackManifest { files: adopted })?;
    if let Err(error) = write_state_locked(&root.join("launcher_state.json"), &after.to_string()) {
        match previous {
            Some(manifest) => {
                let _ = write_manifest(&game, &manifest);
            }
            None => {
                let _ = fs::remove_file(game.join(".openclient").join("modpack.json"));
            }
        }
        return Err(error);
    }
    Ok(json!({"instances": after["instances"], "adopted": adopted_count, "missing": missing}))
}

// ---------------------------------------------------------------- modpack_update

enum UpdateSource {
    Version(String),
    File { path: PathBuf, official: Option<String> },
}

fn update_source(request: &Value) -> Result<UpdateSource, String> {
    match (&request["version"], &request["file"]) {
        (Value::String(version), Value::Null) if request["official"].is_null() => {
            if !valid_id(version) {
                return Err("Identificador de versión inválido.".into());
            }
            Ok(UpdateSource::Version(version.clone()))
        }
        (Value::Null, Value::String(file)) => Ok(UpdateSource::File {
            path: local_pack_path(file)?,
            official: official_id(&request["official"])?,
        }),
        _ => Err("Indica la versión de Modrinth o un archivo .mrpack.".into()),
    }
}

/// `modpack_update` `{root,id,version}` (Modrinth) o `{root,id,file,official?}` →
/// `{"instances":[…],"added","replaced","removed","kept":[…≤50],"keptCount","modifiedCount",
/// "snapshot","loaderChanged","worldBackup"?|"worldBackupSkipped"?}`. Nada cambia en la instancia
/// hasta que todo está en staging; antes de aplicar se copian los mundos (≤ 2 GiB).
pub fn update(request: &Value, ctx: &GameContext) -> Result<Value, String> {
    let id = instance_id(request)?;
    let source = update_source(request)?;
    let root = ctx.root.clone();
    modpack_tx::recover(&root, id)?;
    let state = load_library(&root)?;
    let entry = find_entry(&state, id)?.clone();
    let link = entry["modpack"].clone();
    if !link.is_object() {
        return Err(UNLINKED.into());
    }
    let (instances, game) = locate(&root, id)?;
    let old = read_manifest(&game)?.ok_or(UNLINKED)?;

    ctx.progress("downloading", 0, "Descargando el modpack");
    let (mut pack, kind, project, version, number, name) = match &source {
        UpdateSource::Version(version) => {
            let project = link["projectId"]
                .as_str()
                .filter(|project| link["source"] == "modrinth" && valid_id(project))
                .ok_or("Este modpack no se actualiza desde Modrinth.")?
                .to_owned();
            let (pack, found) = runtime()?.block_on(fetch_modrinth_pack(&project, version, ctx))?;
            let number = found["version_number"].as_str().unwrap_or_default().to_owned();
            let name = link["name"].as_str().unwrap_or_default().to_owned();
            (pack, "modrinth", Some(project), Some(version.clone()), number, name)
        }
        UpdateSource::File { path, official } => {
            let expected = if official.is_some() { "official" } else { "file" };
            if link["source"] != expected
                || official
                    .as_deref()
                    .is_some_and(|official| entry["official"].as_str() != Some(official))
            {
                return Err("Este archivo no corresponde al modpack vinculado a la instancia.".into());
            }
            let pack = Modpack::open_local(path)?;
            let (name, number) = pack.identity()?;
            let (name, number) = (name.to_owned(), number.to_owned());
            if official.is_none() && link["name"].as_str() != Some(clean(&name, 64).as_str()) {
                return Err("El archivo pertenece a otro modpack; no se puede usar para actualizar este.".into());
            }
            (pack, expected, None, None, number, name)
        }
    };
    let dependencies = pack.pack_dependencies()?;
    let loader_changed = entry["mcVersion"] != json!(dependencies.minecraft)
        || entry["category"] != json!(dependencies.loader.category());
    if loader_changed
        && entry["installedContent"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    {
        return Err("Quita el contenido de la instancia antes de cambiar su versión o cargador.".into());
    }
    let new_link = build_link(kind, project.as_deref(), version.as_deref(), &number, &name, &dependencies)?;
    let mut next = entry.clone();
    apply_pack(&mut next, &dependencies, &new_link)?;
    let after = replace_entry(&state, id, &next)?;
    let managed = read_managed(&game)?;

    let staging = Staging::new(&instances)?;
    let staged = runtime()?.block_on(modpack_tx::stage(
        &mut pack,
        &staging.path,
        &modpack::client()?,
        ctx,
    ))?;
    ctx.cancel.check()?;
    ctx.progress("planning", 100, "Comparando con los archivos de la instancia");
    let plan = modpack_tx::plan(&game, &old, &pack, &staged, &managed)?;
    ctx.cancel.check()?;
    let worlds = crate::worlds::auto(ctx, id)?;
    ctx.cancel.check()?;
    let snapshot = modpack_tx::apply_with(&root, id, &plan, &staging.path, &entry, &after.to_string(), ctx)?;
    let mut result = json!({
        "instances": after["instances"],
        "added": plan.add.len(), "replaced": plan.replace.len(), "removed": plan.remove.len(),
        "kept": plan.kept.iter().take(LISTED).collect::<Vec<_>>(), "keptCount": plan.kept.len(),
        "modifiedCount": plan.modified.len(),
        "snapshot": snapshot.id, "loaderChanged": loader_changed
    });
    if let (Some(extra), Some(result)) = (worlds.as_object(), result.as_object_mut()) {
        result.extend(extra.clone());
    }
    Ok(result)
}

// ------------------------------------------------- modpack_snapshots y restore

/// `modpack_snapshots` `{root,id}` → `[{"id","created","from","to","bytes"}]`, la más reciente primero.
/// Solo lectura.
pub fn snapshots(request: &Value) -> Result<Value, String> {
    let root = operation::root_of(request)?;
    let id = instance_id(request)?;
    find_entry(&load_library(&root)?, id)?;
    if locate(&root, id).is_err() {
        return Ok(json!([]));
    }
    let list = modpack_tx::list_snapshots(&root, id)?;
    Ok(Value::Array(
        list.iter()
            .map(|info| {
                json!({
                    "id": info.id, "created": info.created, "from": info.from,
                    "to": info.to, "bytes": info.bytes
                })
            })
            .collect(),
    ))
}

/// `modpack_restore` `{root,id,snapshot}` → `{"instances":[…],"snapshot":"<id nuevo>","keptCount","kept"}`.
/// Antes de restaurar se guarda otra copia del estado actual. Operación registrada, no cancelable.
pub fn restore(request: &Value, ctx: &GameContext) -> Result<Value, String> {
    let id = instance_id(request)?;
    let snapshot = request["snapshot"]
        .as_str()
        .filter(|snapshot| valid_snapshot_id(snapshot))
        .ok_or("Identificador de copia de seguridad inválido.")?;
    let root = ctx.root.clone();
    modpack_tx::recover(&root, id)?;
    let mut instances = Value::Null;
    let info = modpack_tx::restore_with(
        &root,
        id,
        snapshot,
        |saved| {
            let mut state = load_library(&root)?;
            let slot = state["instances"]
                .as_array_mut()
                .and_then(|items| items.iter_mut().find(|item| item["id"].as_str() == Some(id)))
                .ok_or("La instancia ya no existe.")?;
            modpack_tx::restore_entry_fields(slot, saved);
            crate::portable_modpack(&slot["modpack"], &|value: &Value| {
                value.as_str().is_some_and(valid_id)
            })?;
            instances = state["instances"].clone();
            Ok(state.to_string())
        },
        ctx,
    )?;
    Ok(json!({
        "instances": instances, "snapshot": info.id, "keptCount": info.kept.len(),
        "kept": info.kept.iter().take(LISTED).collect::<Vec<_>>()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }

    /// Zip sin compresión (ffi no depende de `zip`): lo justo para un `.mrpack` de pruebas.
    fn stored_zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in entries {
            let offset = out.len() as u32;
            let crc = crc32(data);
            let mut header = Vec::new();
            header.extend_from_slice(&20u16.to_le_bytes()); // versión necesaria
            header.extend_from_slice(&0u16.to_le_bytes()); // flags
            header.extend_from_slice(&0u16.to_le_bytes()); // método: stored
            header.extend_from_slice(&0u16.to_le_bytes()); // hora
            header.extend_from_slice(&0x21u16.to_le_bytes()); // fecha
            header.extend_from_slice(&crc.to_le_bytes());
            header.extend_from_slice(&(data.len() as u32).to_le_bytes());
            header.extend_from_slice(&(data.len() as u32).to_le_bytes());
            header.extend_from_slice(&(name.len() as u16).to_le_bytes());
            header.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&header);
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(data);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes()); // versión de origen
            central.extend_from_slice(&header);
            central.extend_from_slice(&0u16.to_le_bytes()); // comentario
            central.extend_from_slice(&0u16.to_le_bytes()); // disco
            central.extend_from_slice(&0u16.to_le_bytes()); // atributos internos
            central.extend_from_slice(&0u32.to_le_bytes()); // atributos externos
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let start = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    /// `.mrpack` sintético: sin descargas, solo `overrides/`.
    fn write_pack(path: &Path, name: &str, version: &str, minecraft: &str, files: &[(&str, &str)]) {
        let index = json!({
            "formatVersion": 1, "game": "minecraft", "versionId": version, "name": name,
            "files": [], "dependencies": {"minecraft": minecraft, "fabric-loader": "0.16.14"}
        });
        let mut entries = vec![("modrinth.index.json".to_owned(), index.to_string().into_bytes())];
        for (relative, content) in files {
            entries.push((format!("overrides/{relative}"), content.as_bytes().to_vec()));
        }
        fs::write(path, stored_zip(&entries)).unwrap();
    }

    fn tree(directory: &Path) -> BTreeMap<String, Vec<u8>> {
        fn walk(base: &Path, directory: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                let relative = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
                if path.is_dir() {
                    out.insert(format!("{relative}/"), Vec::new());
                    walk(base, &path, out);
                } else {
                    out.insert(relative, fs::read(&path).unwrap());
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(directory, directory, &mut out);
        out
    }

    struct Library {
        base: PathBuf,
        root: PathBuf,
        id: String,
    }

    impl Library {
        fn new(content: Value, extra: &[(&str, &str)]) -> Self {
            let base = std::env::temp_dir().join(format!("openclient-modpack-ops-{}", uuid::Uuid::new_v4()));
            let root = base.join("datos");
            let id = uuid::Uuid::new_v4().to_string();
            let game = root.join("instances").join(&id);
            fs::create_dir_all(&game).unwrap();
            for (relative, text) in extra {
                let path = game.join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, text).unwrap();
            }
            let entry = json!({
                "id": id, "name": "Mi instancia", "mcVersion": "1.21.4", "category": "fabric",
                "loaderLabel": "Fabric", "fabricVersion": "0.16.14", "installedContent": content,
                "ramMb": 4096, "notes": "privado", "modpackVersion": "viejo"
            });
            fs::write(root.join("launcher_state.json"), json!({"instances": [entry]}).to_string()).unwrap();
            Self { base, root, id }
        }

        fn game(&self) -> PathBuf {
            self.root.join("instances").join(&self.id)
        }

        fn pack(&self, name: &str, label: &str, version: &str, files: &[(&str, &str)]) -> PathBuf {
            let path = self.base.join(format!("{label}.mrpack"));
            write_pack(&path, name, version, "1.21.4", files);
            path
        }

        fn call(&self, action: &str, extra: Value) -> Result<Value, String> {
            let mut request = json!({"action": action, "root": self.root, "id": self.id});
            for (key, value) in extra.as_object().unwrap() {
                request[key] = value.clone();
            }
            crate::launcher_operation(&request)
        }

        fn state(&self) -> Value {
            serde_json::from_str(&fs::read_to_string(self.root.join("launcher_state.json")).unwrap()).unwrap()
        }

        fn read(&self, relative: &str) -> Option<String> {
            fs::read_to_string(self.game().join(relative)).ok()
        }
    }

    impl Drop for Library {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn cached_check_honours_ttl_and_the_force_floor() {
        let start = Instant::now();
        let mut checks = Checks::new();
        assert!(cached_in(&checks, "ProyectoA", false, start).is_none());
        checks.insert("ProyectoA".into(), (start, json!([{"id": "v1"}])));
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        assert_eq!(cached_in(&checks, "ProyectoA", false, at(0)).unwrap()[0]["id"], "v1");
        assert!(cached_in(&checks, "ProyectoA", false, at(599)).is_some());
        assert!(cached_in(&checks, "ProyectoA", false, at(600)).is_none());
        // `force` solo se salta la caché si la última consulta tiene 30 s o más.
        assert!(cached_in(&checks, "ProyectoA", true, at(29)).is_some());
        assert!(cached_in(&checks, "ProyectoA", true, at(30)).is_none());
        assert!(cached_in(&checks, "ProyectoB", false, at(1)).is_none());

        store_check("ProyectoTtl", json!([]), start);
        assert!(cached_check("ProyectoTtl", false, at(5)).is_some());
        assert!(cached_check("ProyectoTtl", true, at(31)).is_none());
        assert!(cached_check("ProyectoTtl", false, at(601)).is_none());
    }

    #[test]
    fn check_answers_from_cache_without_network_and_reports_unlinked_shapes() {
        let library = Library::new(json!([]), &[]);
        let unlinked = library.call("modpack_check", json!({})).unwrap();
        assert_eq!(unlinked["linked"], false);
        assert_eq!(unlinked["legacy"], true);
        assert_eq!(unlinked["manifest"], false);

        let release = |id: &str, number: &str, kind: &str, loaders: Value| {
            json!({"id": id, "number": number, "channel": kind, "date": "2026-01-01",
                "minecraft": ["1.21.4"], "loaders": loaders})
        };
        store_check(
            "ProyectoCheck",
            json!([
                release("beta2", "2.0-beta", "beta", json!(["fabric"])),
                release("quilt2", "2.0", "release", json!(["quilt"])),
                release("fab2", "1.5", "release", json!(["fabric"])),
                release("fab1", "1.4.2", "release", json!(["fabric"])),
            ]),
            Instant::now(),
        );
        let mut state = library.state();
        state["instances"][0]["modpack"] = json!({"source": "modrinth", "projectId": "ProyectoCheck",
            "versionId": "fab1", "versionNumber": "1.4.2", "name": "Mi Pack", "loader": "fabric",
            "loaderVersion": "0.16.14"});
        fs::write(library.root.join("launcher_state.json"), state.to_string()).unwrap();
        for force in [false, true] {
            let linked = library.call("modpack_check", json!({"force": force})).unwrap();
            assert_eq!(linked["linked"], true);
            assert_eq!(linked["latest"]["id"], "fab2");
            assert_eq!(linked["updateAvailable"], true);
            assert_eq!(linked["versions"].as_array().unwrap().len(), 4);
            assert_eq!(linked["current"]["versionNumber"], "1.4.2");
            assert_eq!(linked["manifest"], false);
        }
        state["instances"][0]["modpack"]["versionId"] = json!("fab2");
        fs::write(library.root.join("launcher_state.json"), state.to_string()).unwrap();
        assert_eq!(library.call("modpack_check", json!({})).unwrap()["updateAvailable"], false);

        state["instances"][0]["modpack"]["source"] = json!("file");
        fs::write(library.root.join("launcher_state.json"), state.to_string()).unwrap();
        let file = library.call("modpack_check", json!({})).unwrap();
        assert_eq!((file["linked"].clone(), file["source"].clone(), file["updateAvailable"].clone()),
            (json!(true), json!("file"), json!(false)));
        assert_eq!(file["versions"], json!([]));
        assert!(library.call("modpack_check", json!({"id": "../x"})).is_err());
    }

    #[test]
    fn requests_are_validated_before_network_or_disk() {
        let library = Library::new(json!([]), &[("config/a.txt", "x")]);
        let before = tree(&library.base);
        let fail = |action: &str, extra: Value| library.call(action, extra).unwrap_err();
        assert_eq!(fail("modpack_update", json!({"id": "no-uuid", "version": "abc"})), "Identificador de instancia inválido.");
        assert_eq!(fail("modpack_update", json!({"version": "a b"})), "Identificador de versión inválido.");
        assert_eq!(fail("modpack_update", json!({"version": "x".repeat(33)})), "Identificador de versión inválido.");
        assert_eq!(fail("modpack_update", json!({})), "Indica la versión de Modrinth o un archivo .mrpack.");
        assert_eq!(fail("modpack_update", json!({"version": "abc", "file": "C:\\x.mrpack"})),
            "Indica la versión de Modrinth o un archivo .mrpack.");
        assert_eq!(fail("modpack_update", json!({"file": "relativo.mrpack"})), "Selecciona un archivo .mrpack.");
        assert_eq!(fail("modpack_update", json!({"file": library.base.join("x.zip")})), "Selecciona un archivo .mrpack.");
        assert_eq!(fail("modpack_update", json!({"file": library.base.join("x.mrpack"), "official": "../x"})),
            "Identificador del modpack oficial inválido.");
        assert_eq!(fail("modpack_update", json!({"version": "abc", "official": "diosesmc"})),
            "Indica la versión de Modrinth o un archivo .mrpack.");
        assert_eq!(fail("modpack_link", json!({"project": "a b", "version": "abc"})), "Proyecto inválido.");
        assert_eq!(fail("modpack_link", json!({"project": "abc", "version": "../1"})), "Identificador de versión inválido.");
        assert_eq!(fail("modpack_link", json!({"project": "abc"})), "Indica el modpack de Modrinth o un archivo .mrpack.");
        assert_eq!(fail("modpack_link", json!({"file": "relativo.mrpack"})), "Selecciona un archivo .mrpack.");
        // Un archivo de pack inexistente falla al abrirse, no antes de validar el vínculo.
        assert!(fail("modpack_update", json!({"file": library.base.join("falta.mrpack")})).contains("Vincula"));
        assert_eq!(tree(&library.base), before);
    }

    #[test]
    fn snapshot_ids_are_validated_before_touching_the_library() {
        for good in ["1-0123abcd", "1767225600000-deadbeef", &format!("{}-ffffffff", "9".repeat(20))] {
            assert!(valid_snapshot_id(good), "{good}");
        }
        let long = format!("{}-ffffffff", "9".repeat(21));
        for bad in [
            "", "-", "1-", "-0123abcd", "../x", "..\\x", "1-zzzzzzzz", "1-ABCDEF12", "1-0123abc", "1-0123abcde",
            "a-0123abcd", "1/0123abcd", "1-0123abcd/..", "1-0123abcd\n", long.as_str(), "1-0123abcd ", "C:\\x",
        ] {
            assert!(!valid_snapshot_id(bad), "{bad:?}");
        }
        let library = Library::new(json!([]), &[("options.txt", "fov:90")]);
        let before = tree(&library.base);
        for bad in ["../x", "1-zzzzzzzz", "", "1-0123abcd/../.."] {
            let error = library.call("modpack_restore", json!({"snapshot": bad})).unwrap_err();
            assert_eq!(error, "Identificador de copia de seguridad inválido.", "{bad}");
        }
        assert!(library.call("modpack_restore", json!({})).is_err());
        // Un id bien formado que no existe falla sin crear nada.
        assert_eq!(
            library.call("modpack_restore", json!({"snapshot": "1-0123abcd"})).unwrap_err(),
            "La copia de seguridad ya no existe."
        );
        assert_eq!(library.call("modpack_snapshots", json!({})).unwrap(), json!([]));
        assert_eq!(tree(&library.base), before);
    }

    #[test]
    fn link_adopts_only_files_whose_sha512_matches_exactly() {
        let library = Library::new(
            json!([]),
            &[
                ("mods/a.jar", "A"),
                ("config/b.txt", "B"),
                ("mods/c.jar", "C-modificado"),
                ("mods/usuario.jar", "mio"),
            ],
        );
        let pack = library.pack(
            "Mi Pack",
            "v1",
            "1.0.0",
            &[("mods/a.jar", "A"), ("config/b.txt", "B"), ("mods/c.jar", "C"), ("mods/nuevo.jar", "N")],
        );
        let before = tree(&library.game());
        let result = library.call("modpack_link", json!({"file": pack})).unwrap();
        assert_eq!((result["adopted"].clone(), result["missing"].clone()), (json!(2), json!(2)));
        let manifest = read_manifest(&library.game()).unwrap().unwrap();
        let mut adopted: Vec<&str> = manifest.files.iter().map(|file| file.path.as_str()).collect();
        adopted.sort_unstable();
        assert_eq!(adopted, ["config/b.txt", "mods/a.jar"]);
        // Solo se añadió el sidecar: ningún archivo de la instancia cambió y no queda staging.
        let mut after = tree(&library.game());
        assert!(after.remove(".openclient/modpack.json").is_some());
        after.remove(".openclient/");
        assert_eq!(after, before);
        assert_eq!(tree(&library.root.join("instances")).keys().filter(|key| key.contains("stage")).count(), 0);
        let entry = &result["instances"][0];
        assert_eq!(entry["modpack"]["source"], "file");
        assert_eq!(entry["modpack"]["name"], "Mi Pack");
        assert_eq!(entry["modpack"]["versionNumber"], "1.0.0");
        assert_eq!(entry["modpack"]["loader"], "fabric");
        assert_eq!(entry["modpackVersion"], "1.0.0");
        assert_eq!(library.state()["instances"][0]["ramMb"], 4096);
        let check = library.call("modpack_check", json!({})).unwrap();
        assert_eq!((check["linked"].clone(), check["manifest"].clone()), (json!(true), json!(true)));

        // Otra versión de Minecraft o de cargador no se vincula, y no cambia nada.
        let other = library.base.join("otro.mrpack");
        write_pack(&other, "Mi Pack", "1.0.0", "1.20.1", &[("mods/a.jar", "A")]);
        let snapshot = tree(&library.base);
        assert!(library.call("modpack_link", json!({"file": other})).unwrap_err().contains("no coincide"));
        assert_eq!(tree(&library.base), snapshot);
    }

    #[test]
    fn update_flow_end_to_end_with_snapshot_restore_and_untouched_instance_on_failure() {
        let library = Library::new(
            json!([{"id": "abc", "versionId": "v1", "type": "mod", "enabled": true}]),
            &[
                ("mods/a.jar", "A1"),
                ("mods/b.jar", "B1"),
                ("config/c.txt", "C1"),
                ("mods/usuario.jar", "mio"),
                ("saves/w/level.dat", "MUNDO"),
            ],
        );
        let v1 = library.pack("Mi Pack", "v1", "1.0.0", &[("mods/a.jar", "A1"), ("mods/b.jar", "B1"), ("config/c.txt", "C1")]);
        let linked = library.call("modpack_link", json!({"file": v1})).unwrap();
        assert_eq!((linked["adopted"].clone(), linked["missing"].clone()), (json!(3), json!(0)));
        fs::write(library.game().join("config/c.txt"), "mio").unwrap();

        let v2 = library.pack(
            "Mi Pack",
            "v2",
            "2.0.0",
            &[("mods/b.jar", "B2"), ("mods/n.jar", "N1"), ("config/c.txt", "C2"), ("saves/w/level.dat", "NUEVO")],
        );

        // Fallos antes de aplicar: la instancia, la biblioteca y las carpetas quedan como estaban.
        let untouched = |library: &Library| (tree(&library.base), library.state());
        let other = library.pack("Otro Pack", "otro", "9.0.0", &[("mods/z.jar", "Z")]);
        let moved = library.base.join("mc120.mrpack");
        write_pack(&moved, "Mi Pack", "3.0.0", "1.20.1", &[("mods/z.jar", "Z")]);
        let baseline = untouched(&library);
        assert!(library.call("modpack_update", json!({"file": other})).unwrap_err().contains("otro modpack"));
        assert!(library.call("modpack_update", json!({"file": moved})).unwrap_err().contains("Quita el contenido"));
        assert!(library
            .call("modpack_update", json!({"file": v2, "official": "diosesmc"}))
            .unwrap_err()
            .contains("no corresponde"));
        assert_eq!(untouched(&library), baseline);

        let cancelled = openclient_core::CancelToken::default();
        cancelled.cancel();
        let request = json!({"action": "modpack_update", "root": library.root, "id": library.id, "file": v2});
        let context = operation::detached_context(&request).unwrap().with_cancel(cancelled);
        assert!(update(&request, &context).unwrap_err().starts_with("CANCELLED:"));
        assert_eq!(untouched(&library), baseline);

        fs::write(library.game().join("mods/.openclient_managed.json"), r#"["b.jar"]"#).unwrap();
        let baseline = untouched(&library);
        let error = library.call("modpack_update", json!({"file": v2})).unwrap_err();
        assert!(error.contains("choca con contenido añadido: b.jar"), "{error}");
        assert_eq!(untouched(&library), baseline);
        fs::remove_file(library.game().join("mods/.openclient_managed.json")).unwrap();

        // Actualización real.
        let done = library.call("modpack_update", json!({"file": v2})).unwrap();
        assert_eq!(
            (done["added"].clone(), done["replaced"].clone(), done["removed"].clone(), done["keptCount"].clone()),
            (json!(1), json!(1), json!(1), json!(2))
        );
        assert_eq!(done["kept"], json!(["config/c.txt", "saves/w/level.dat"]));
        assert_eq!(done["loaderChanged"], false);
        let backups = library.call("world_list", json!({})).unwrap();
        assert_eq!(backups["backups"][0]["id"], done["worldBackup"]);
        assert_eq!(backups["backups"][0]["worlds"], json!(["w"]));
        assert_eq!(done["instances"][0]["modpack"]["versionNumber"], "2.0.0");
        assert_eq!(done["instances"][0]["modpackVersion"], "2.0.0");
        assert_eq!(done["instances"][0]["ramMb"], 4096);
        assert_eq!(done["instances"][0]["notes"], "privado");
        assert_eq!(library.state()["instances"], done["instances"]);
        assert_eq!(library.read("mods/a.jar"), None);
        assert_eq!(library.read("mods/b.jar").as_deref(), Some("B2"));
        assert_eq!(library.read("mods/n.jar").as_deref(), Some("N1"));
        assert_eq!(library.read("mods/usuario.jar").as_deref(), Some("mio"));
        assert_eq!(library.read("config/c.txt").as_deref(), Some("mio"));
        assert_eq!(library.read("saves/w/level.dat").as_deref(), Some("MUNDO"));
        assert_eq!(read_manifest(&library.game()).unwrap().unwrap().files.len(), 2);
        assert!(!tree(&library.root.join("instances")).keys().any(|key| key.contains("stage")));
        assert!(!library.root.join("snapshots").join(&library.id).join(".applying").exists());

        // Copias de seguridad y restauración.
        let listed = library.call("modpack_snapshots", json!({})).unwrap();
        assert_eq!(listed.as_array().unwrap().len(), 1);
        assert_eq!((listed[0]["from"].clone(), listed[0]["to"].clone()), (json!("1.0.0"), json!("2.0.0")));
        assert_eq!(listed[0]["id"], done["snapshot"]);
        let restored = library.call("modpack_restore", json!({"snapshot": done["snapshot"]})).unwrap();
        assert_eq!(restored["instances"][0]["modpackVersion"], "1.0.0");
        assert_eq!(restored["instances"][0]["modpack"]["versionNumber"], "1.0.0");
        assert_eq!(restored["instances"][0]["ramMb"], 4096);
        assert_ne!(restored["snapshot"], done["snapshot"]);
        assert_eq!(library.read("mods/a.jar").as_deref(), Some("A1"));
        assert_eq!(library.read("mods/b.jar").as_deref(), Some("B1"));
        assert_eq!(library.read("mods/n.jar"), None);
        assert_eq!(library.read("config/c.txt").as_deref(), Some("mio"));
        assert_eq!(library.read("saves/w/level.dat").as_deref(), Some("MUNDO"));
        assert_eq!(read_manifest(&library.game()).unwrap().unwrap().files.len(), 3);
        assert_eq!(library.state()["instances"], restored["instances"]);
        // Restaurar también se puede deshacer: ahora hay dos copias.
        assert_eq!(library.call("modpack_snapshots", json!({})).unwrap().as_array().unwrap().len(), 2);
    }
}
