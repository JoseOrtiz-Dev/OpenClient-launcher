//! Piezas compartidas para identificar archivos de contenido en Modrinth por hash: hash por
//! streaming, validación de `/version_files`, lectura acotada de `/projects` y `/teams`, y
//! las consultas HTTP en lote. Las usan la exportación `.mrpack` y la lista de contenido.

use serde_json::{json, Map, Value};
use sha1::Sha1;
use sha2::{Digest, Sha512};
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::Path,
};

pub const API: &str = "https://api.modrinth.com/v2";
const RESPONSE_LIMIT: usize = 64 * 1024 * 1024;
const NETWORK_ERROR: &str = "No se pudo consultar Modrinth.";
/// Longitud máxima de cualquier texto que viaja a la interfaz.
pub const TEXT_LIMIT: usize = 128;

/// Enlace simbólico o, en Windows, cualquier punto de reanálisis (junction, etc.).
pub fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink()
}

/// `(sha1, sha512, tamaño)` leyendo el archivo una sola vez, por bloques.
pub fn hash_file(path: &Path) -> Result<(String, String, u64), String> {
    let mut input = fs::File::open(path).map_err(|error| error.to_string())?;
    let (mut sha1, mut sha512) = (Sha1::new(), Sha512::new());
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        sha1.update(&buffer[..read]);
        sha512.update(&buffer[..read]);
        total += read as u64;
    }
    Ok((format!("{:x}", sha1.finalize()), format!("{:x}", sha512.finalize()), total))
}

pub fn hex_valid(text: &str, length: usize) -> bool {
    text.len() == length && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Identificador de proyecto/versión/equipo de Modrinth.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// Sin caracteres de control, sin espacios en los extremos y como mucho `TEXT_LIMIT` caracteres.
pub fn clean_text(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_control())
        .take(TEXT_LIMIT)
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Icono servido por el CDN de Modrinth (`https://cdn.modrinth.com/data/...`).
pub fn cdn_icon(url: &str) -> Option<&str> {
    let parsed = reqwest::Url::parse(url).ok()?;
    (parsed.scheme() == "https"
        && parsed.host_str() == Some("cdn.modrinth.com")
        && parsed.port().is_none()
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.path().starts_with("/data/"))
    .then_some(url)
}

/// Resultado validado de buscar un archivo en la respuesta de `POST /version_files`.
pub struct Verified<'a> {
    pub project: &'a str,
    pub version: &'a Value,
    pub file: &'a Value,
}

/// Acepta la versión asociada a `sha512` solo si el proyecto es válido y el archivo que
/// declara coincide en sha512, sha1 y tamaño con el local.
pub fn verified_file<'a>(
    versions: &'a Map<String, Value>,
    sha1: &str,
    sha512: &str,
    size: u64,
) -> Option<Verified<'a>> {
    let version = versions.get(sha512)?;
    let project = version["project_id"].as_str().filter(|id| valid_id(id))?;
    let file = version["files"].as_array()?.iter().find(|file| {
        file["hashes"]["sha512"]
            .as_str()
            .is_some_and(|hash| hash.eq_ignore_ascii_case(sha512))
    })?;
    let same_sha1 = file["hashes"]["sha1"]
        .as_str()
        .is_some_and(|hash| hash.eq_ignore_ascii_case(sha1));
    (same_sha1 && file["size"].as_u64() == Some(size)).then_some(Verified {
        project,
        version,
        file,
    })
}

/// Datos de un proyecto de `GET /projects` (ya saneados).
#[derive(Debug, PartialEq, Eq)]
pub struct ProjectInfo {
    pub title: String,
    /// Solo si pertenece al CDN de Modrinth.
    pub icon: Option<String>,
    pub team: Option<String>,
}

pub fn project_infos(response: &Value) -> HashMap<String, ProjectInfo> {
    response
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|project| {
            let id = project["id"].as_str().filter(|id| valid_id(id))?;
            Some((
                id.to_owned(),
                ProjectInfo {
                    title: clean_text(project["title"].as_str().unwrap_or_default()),
                    icon: project["icon_url"].as_str().and_then(cdn_icon).map(str::to_owned),
                    team: project["team"].as_str().filter(|id| valid_id(id)).map(str::to_owned),
                },
            ))
        })
        .collect()
}

/// Autor por equipo a partir de `GET /teams?ids=`: el miembro `Owner` o, si no hay, el de
/// menor `ordering`.
pub fn team_authors(response: &Value) -> HashMap<String, String> {
    response
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|members| {
            let members = members.as_array()?;
            let team = members.first()?["team_id"].as_str().filter(|id| valid_id(id))?;
            let author = members
                .iter()
                .find(|member| member["role"] == "Owner")
                .or_else(|| members.iter().min_by_key(|member| member["ordering"].as_i64().unwrap_or(i64::MAX)))?;
            let name = clean_text(author["user"]["username"].as_str()?);
            (!name.is_empty()).then(|| (team.to_owned(), name))
        })
        .collect()
}

async fn read_json(response: reqwest::Response) -> Result<Value, String> {
    use futures_util::TryStreamExt;
    if !response.status().is_success() {
        return Err(NETWORK_ERROR.into());
    }
    let mut chunks = response.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = chunks.try_next().await.map_err(|_| NETWORK_ERROR)? {
        if body.len() + chunk.len() > RESPONSE_LIMIT {
            return Err(NETWORK_ERROR.into());
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| NETWORK_ERROR.into())
}

async fn send(request: reqwest::RequestBuilder) -> Result<Value, String> {
    read_json(request.send().await.map_err(|_| NETWORK_ERROR)?).await
}

/// `POST /version_files` por sha512 (el llamador limita el lote a 500).
pub async fn post_version_files(
    client: &reqwest::Client,
    api: &str,
    hashes: &[String],
) -> Result<Value, String> {
    send(client.post(format!("{api}/version_files")).json(&json!({"hashes": hashes, "algorithm": "sha512"}))).await
}

/// `GET /projects?ids=[...]` (el llamador limita el lote a 100).
pub async fn get_projects(client: &reqwest::Client, api: &str, ids: &[String]) -> Result<Value, String> {
    let ids = serde_json::to_string(ids).map_err(|error| error.to_string())?;
    send(client.get(format!("{api}/projects")).query(&[("ids", ids)])).await
}

/// `GET /teams?ids=[...]` (el llamador limita el lote a 100).
pub async fn get_teams(client: &reqwest::Client, api: &str, ids: &[String]) -> Result<Value, String> {
    let ids = serde_json::to_string(ids).map_err(|error| error.to_string())?;
    send(client.get(format!("{api}/teams")).query(&[("ids", ids)])).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_file_requires_matching_hashes_size_and_project() {
        let mut versions = Map::new();
        let version = |sha1: &str, size: u64, project: &str| {
            json!({"project_id": project, "files": [{"hashes": {"sha1": sha1, "sha512": "ABC"}, "size": size}]})
        };
        versions.insert("abc".into(), version("DEF", 10, "P1"));
        assert_eq!(verified_file(&versions, "def", "abc", 10).unwrap().project, "P1");
        assert!(verified_file(&versions, "000", "abc", 10).is_none());
        assert!(verified_file(&versions, "def", "abc", 11).is_none());
        assert!(verified_file(&versions, "def", "zzz", 10).is_none());
        versions.insert("abc".into(), version("DEF", 10, "../x"));
        assert!(verified_file(&versions, "def", "abc", 10).is_none());
    }

    #[test]
    fn projects_keep_only_cdn_icons_and_teams_pick_the_owner() {
        let infos = project_infos(&json!([
            {"id": "P1", "title": "Uno\n", "icon_url": "https://cdn.modrinth.com/data/P1/icon.png", "team": "T1"},
            {"id": "P2", "title": "Dos", "icon_url": "https://evil.example/data/icon.png"},
            {"id": "bad id", "title": "x"}
        ]));
        assert_eq!(infos.len(), 2);
        assert_eq!(infos["P1"].title, "Uno");
        assert_eq!(infos["P1"].icon.as_deref(), Some("https://cdn.modrinth.com/data/P1/icon.png"));
        assert_eq!(infos["P1"].team.as_deref(), Some("T1"));
        assert_eq!(infos["P2"].icon, None);
        let authors = team_authors(&json!([
            [{"team_id": "T1", "role": "Member", "ordering": 0, "user": {"username": "otro"}},
             {"team_id": "T1", "role": "Owner", "ordering": 5, "user": {"username": "dueno"}}],
            [{"team_id": "T2", "role": "Member", "ordering": 3, "user": {"username": "tres"}},
             {"team_id": "T2", "role": "Member", "ordering": 1, "user": {"username": "uno"}}],
            [{"team_id": "../x", "role": "Owner", "user": {"username": "no"}}]
        ]));
        assert_eq!(authors.len(), 2);
        assert_eq!(authors["T1"], "dueno");
        assert_eq!(authors["T2"], "uno");
    }
}
