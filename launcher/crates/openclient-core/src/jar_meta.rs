//! Metadatos internos de jars de mods y zips de resourcepacks, leídos sin extraer nada a disco.
//!
//! Solo se abren rutas fijas con `by_name`. Todo está acotado: archivo ≤ 256 MiB, ≤ 20 000
//! entradas, textos ≤ 64 KiB (tanto el tamaño declarado como lo realmente leído) e iconos PNG
//! ≤ 256 KiB y ≤ 512×512. Cualquier error significa «sin metadatos», nunca un fallo.

use crate::content_meta::{clean_text, is_link};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Seek},
    path::Path,
};
use zip::ZipArchive;

const ARCHIVE_LIMIT: u64 = 256 * 1024 * 1024;
const ENTRY_LIMIT: usize = 20_000;
const TEXT_BYTES: u64 = 64 * 1024;
const MANIFEST_BYTES: u64 = 16 * 1024;
const ICON_BYTES: u64 = 256 * 1024;
const ICON_SIDE: u32 = 512;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Mod,
    ResourcePack,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct JarMeta {
    pub title: Option<String>,
    pub version: Option<String>,
    pub author: Option<String>,
    /// Solo resourcepacks (`pack.mcmeta`), como texto plano sin códigos `§`.
    pub description: Option<String>,
    /// PNG ya validado (firma, tamaño y dimensiones).
    pub icon: Option<Vec<u8>>,
}

impl JarMeta {
    fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.version.is_none()
            && self.author.is_none()
            && self.description.is_none()
            && self.icon.is_none()
    }
}

type Found = (JarMeta, Option<String>);

/// Lee los metadatos de un archivo del disco (`None` si no es un enlace ni un zip legible).
pub fn read(path: &Path, kind: Kind) -> Option<JarMeta> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if is_link(&metadata) || !metadata.is_file() || metadata.len() > ARCHIVE_LIMIT {
        return None;
    }
    read_archive(fs::File::open(path).ok()?, kind)
}

pub fn read_archive<R: Read + Seek>(reader: R, kind: Kind) -> Option<JarMeta> {
    let mut zip = ZipArchive::new(reader).ok()?;
    if zip.len() > ENTRY_LIMIT {
        return None;
    }
    let (mut meta, icon) = match kind {
        Kind::Mod => mod_meta(&mut zip)?,
        Kind::ResourcePack => pack_meta(&mut zip),
    };
    meta.icon = icon.and_then(|path| read_icon(&mut zip, &path));
    (!meta.is_empty()).then_some(meta)
}

/// Quita los códigos de formato de Minecraft (`§` y el carácter que lo sigue).
pub fn strip_format(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character == '§' {
            chars.next();
        } else {
            out.push(character);
        }
    }
    out
}

fn read_text<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str, limit: u64) -> Option<String> {
    let entry = zip.by_name(name).ok()?;
    if entry.size() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    entry.take(limit + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > limit {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn read_json<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Option<Value> {
    let text = read_text(zip, name, TEXT_BYTES)?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

fn text_of(value: &Value) -> Option<String> {
    Some(clean_text(value.as_str()?)).filter(|text| !text.is_empty())
}

/// Una versión con marcadores sin resolver (`${version}`) no sirve.
fn version_of(value: &Value) -> Option<String> {
    text_of(value).filter(|version| !version.contains("${"))
}

fn join_names(names: impl Iterator<Item = String>) -> Option<String> {
    let names: Vec<String> = names.take(3).collect();
    (!names.is_empty()).then(|| clean_text(&names.join(", ")))
}

fn author_list(value: &Value) -> Option<String> {
    join_names(value.as_array()?.iter().filter_map(|author| {
        text_of(if author.is_object() { &author["name"] } else { author })
    }))
}

/// Ruta de icono: texto, o mapa `tamaño -> ruta` del que se elige el más cercano a 64.
fn icon_path(value: &Value) -> Option<String> {
    match value {
        Value::String(path) => Some(path.clone()),
        Value::Object(sizes) => sizes
            .iter()
            .filter_map(|(size, path)| Some((size.parse::<i64>().ok()?, path.as_str()?)))
            .min_by_key(|(size, _)| (size - 64).abs())
            .map(|(_, path)| path.to_owned()),
        _ => None,
    }
}

fn found(title: Option<String>, version: Option<String>, author: Option<String>, icon: Option<String>) -> Option<Found> {
    (title.is_some() || version.is_some() || author.is_some() || icon.is_some()).then(|| {
        (JarMeta { title, version, author, ..JarMeta::default() }, icon)
    })
}

fn fabric(root: &Value) -> Option<Found> {
    found(
        text_of(&root["name"]).or_else(|| text_of(&root["id"])),
        version_of(&root["version"]),
        author_list(&root["authors"]),
        icon_path(&root["icon"]),
    )
}

fn quilt(root: &Value) -> Option<Found> {
    let loader = &root["quilt_loader"];
    let meta = if loader["metadata"].is_object() { &loader["metadata"] } else { &root["metadata"] };
    let mut people: Vec<(&String, &str)> = meta["contributors"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, role)| (name, role.as_str().unwrap_or_default()))
        .collect();
    people.sort_by_key(|(_, role)| *role != "Owner");
    found(
        text_of(&meta["name"]).or_else(|| text_of(&loader["id"])),
        version_of(&loader["version"]),
        join_names(people.into_iter().map(|(name, _)| clean_text(name)).filter(|name| !name.is_empty())),
        icon_path(&meta["icon"]),
    )
}

/// `mcmod.info` antiguo: lista (o `{"modList": [...]}`) cuyo primer objeto describe el mod.
fn legacy(root: &Value) -> Option<Found> {
    let first = root.as_array().or_else(|| root["modList"].as_array())?.first()?;
    found(
        text_of(&first["name"]).or_else(|| text_of(&first["modid"])),
        version_of(&first["version"]),
        author_list(&first["authorList"]).or_else(|| author_list(&first["authors"])),
        text_of(&first["logoFile"]),
    )
}

/// Valor de una cadena TOML en una sola línea; `None` para otros tipos o cadenas multilínea.
fn toml_string(value: &str) -> Option<String> {
    let quote = value.chars().next().filter(|c| matches!(c, '"' | '\''))?;
    let rest = &value[1..];
    if rest.starts_with(quote) && rest[1..].starts_with(quote) {
        return None;
    }
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(character) = chars.next() {
        match character {
            c if c == quote => return Some(out),
            '\\' if quote == '"' => out.extend(chars.next()),
            c => out.push(c),
        }
    }
    None
}

/// Primer `[[mods]]` de `mods.toml`/`neoforge.mods.toml` con un parser mínimo de `clave = "valor"`.
fn toml_mod(text: &str) -> Option<Found> {
    let (mut inside, mut multiline) = (false, None::<&str>);
    let (mut title, mut version, mut author, mut logo) = (None, None, None, None);
    for line in text.lines() {
        if let Some(delimiter) = multiline {
            if line.contains(delimiter) {
                multiline = None;
            }
            continue;
        }
        let line = line.trim();
        if line.starts_with('[') {
            if inside {
                break;
            }
            inside = line.replace(' ', "").starts_with("[[mods]]");
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| inside) else {
            continue;
        };
        let value = value.trim();
        if let Some(delimiter) = ["'''", "\"\"\""].into_iter().find(|d| value.starts_with(d)) {
            multiline = (!value[3..].contains(delimiter)).then_some(delimiter);
        }
        match key.trim() {
            "displayName" => title = toml_string(value),
            "version" => version = toml_string(value),
            "authors" => author = toml_string(value),
            "logoFile" => logo = toml_string(value),
            _ => {}
        }
    }
    found(
        title.map(|t| clean_text(&t)).filter(|t| !t.is_empty()),
        version,
        author.map(|a| clean_text(&a)).filter(|a| !a.is_empty()),
        logo,
    )
}

fn manifest_version<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Option<String> {
    let text = read_text(zip, "META-INF/MANIFEST.MF", MANIFEST_BYTES)?;
    let line = text.lines().find_map(|line| line.strip_prefix("Implementation-Version:"))?;
    Some(clean_text(line)).filter(|version| !version.is_empty() && !version.contains("${"))
}

fn mod_meta<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Option<Found> {
    if let Some(found) = read_json(zip, "fabric.mod.json").and_then(|root| fabric(&root)) {
        return Some(found);
    }
    if let Some(found) = read_json(zip, "quilt.mod.json").and_then(|root| quilt(&root)) {
        return Some(found);
    }
    for name in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        let Some(text) = read_text(zip, name, TEXT_BYTES) else {
            continue;
        };
        if let Some((mut meta, icon)) = toml_mod(&text) {
            if meta.version.as_deref() == Some("${file.jarVersion}") {
                meta.version = manifest_version(zip);
            } else if meta.version.as_deref().is_some_and(|version| version.contains("${")) {
                meta.version = None;
            }
            return Some((meta, icon));
        }
    }
    read_json(zip, "mcmod.info").and_then(|root| legacy(&root))
}

/// Texto plano de un componente de texto de Minecraft (cadena, objeto con `text`/`extra` o lista).
fn plain_text(value: &Value, out: &mut String, depth: u8) {
    if depth > 4 || out.len() > 512 {
        return;
    }
    match value {
        Value::String(text) => out.push_str(text),
        Value::Array(parts) => parts.iter().for_each(|part| plain_text(part, out, depth + 1)),
        Value::Object(_) => {
            plain_text(&value["text"], out, depth + 1);
            plain_text(&value["extra"], out, depth + 1);
        }
        _ => {}
    }
}

fn pack_meta<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Found {
    let description = read_json(zip, "pack.mcmeta").and_then(|root| {
        let mut text = String::new();
        plain_text(&root["pack"]["description"], &mut text, 0);
        Some(clean_text(&strip_format(&text.replace(['\n', '\r'], " ")))).filter(|text| !text.is_empty())
    });
    (JarMeta { description, ..JarMeta::default() }, Some("pack.png".to_owned()))
}

/// Ruta relativa dentro del zip: sin `/` inicial, sin `..`, sin `\`, sin `:` ni controles.
fn icon_name(path: &str) -> Option<String> {
    if path.is_empty()
        || path.len() > 256
        || path.starts_with('/')
        || path.contains(['\\', ':'])
        || path.chars().any(char::is_control)
    {
        return None;
    }
    let parts: Vec<&str> = path.split('/').filter(|part| *part != ".").collect();
    (!parts.is_empty() && parts.iter().all(|part| !part.is_empty() && *part != ".."))
        .then(|| parts.join("/"))
}

fn png_valid(bytes: &[u8]) -> bool {
    let side = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    bytes.starts_with(PNG_SIGNATURE)
        && bytes.get(12..16) == Some(b"IHDR")
        && matches!((side(16), side(20)), (Some(1..=ICON_SIDE), Some(1..=ICON_SIDE)))
}

fn read_icon<R: Read + Seek>(zip: &mut ZipArchive<R>, path: &str) -> Option<Vec<u8>> {
    let entry = zip.by_name(&icon_name(path)?).ok()?;
    if entry.size() > ICON_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    entry.take(ICON_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= ICON_BYTES && png_valid(&bytes)).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

    fn archive(entries: &[(&str, &[u8])], method: CompressionMethod) -> Cursor<Vec<u8>> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in entries {
            zip.start_file(*name, SimpleFileOptions::default().compression_method(method)).unwrap();
            zip.write_all(data).unwrap();
        }
        let mut cursor = zip.finish().unwrap();
        cursor.set_position(0);
        cursor
    }

    fn jar(entries: &[(&str, &[u8])]) -> Cursor<Vec<u8>> {
        archive(entries, CompressionMethod::Stored)
    }

    fn png(side: u32) -> Vec<u8> {
        let mut bytes = PNG_SIGNATURE.to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&side.to_be_bytes());
        bytes.extend_from_slice(&side.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        bytes
    }

    #[test]
    fn fabric_reads_mixed_authors_and_the_icon_closest_to_64() {
        let manifest = br#"{"id":"sodium","name":"Sodium","version":"0.6.0","authors":["jellysquid3",{"name":"IMS"},7],
            "icon":{"128":"big.png","32":"small.png"}}"#;
        let meta = read_archive(
            jar(&[("fabric.mod.json", manifest), ("small.png", &png(32)), ("big.png", &png(128))]),
            Kind::Mod,
        )
        .unwrap();
        assert_eq!(meta.title.as_deref(), Some("Sodium"));
        assert_eq!(meta.version.as_deref(), Some("0.6.0"));
        assert_eq!(meta.author.as_deref(), Some("jellysquid3, IMS"));
        assert_eq!(meta.icon.unwrap(), png(32));
        // Una versión sin resolver y un icono inexistente no producen valores.
        let meta = read_archive(
            jar(&[("fabric.mod.json", br#"{"id":"x","version":"${version}","icon":"nada.png"}"#)]),
            Kind::Mod,
        )
        .unwrap();
        assert_eq!((meta.title.as_deref(), meta.version, meta.icon), (Some("x"), None, None));
    }

    #[test]
    fn quilt_puts_the_owner_first() {
        let manifest = br#"{"quilt_loader":{"id":"q","version":"1.2","metadata":{"name":"Quilted",
            "contributors":{"Bob":"Developer","Alice":"Owner"},"icon":"logo.png"}}}"#;
        let meta = read_archive(jar(&[("quilt.mod.json", manifest), ("logo.png", &png(64))]), Kind::Mod).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Quilted"));
        assert_eq!(meta.version.as_deref(), Some("1.2"));
        assert_eq!(meta.author.as_deref(), Some("Alice, Bob"));
        assert_eq!(meta.icon.unwrap(), png(64));
    }

    #[test]
    fn mods_toml_resolves_the_jar_version_from_the_manifest() {
        let toml = br#"
modLoader="javafml"
authors="no es de este mod"
[[mods]]
modId="ejemplo"
displayName = "Ejemplo Mod" # comentario
version="${file.jarVersion}"
authors = 'Ana, Luis'
logoFile="logo.png"
description='''
displayName = "falso"
'''
[[dependencies.ejemplo]]
displayName = "otra tabla"
"#;
        let manifest = b"Manifest-Version: 1.0\r\nImplementation-Version: 1.2.3\r\n";
        let meta = read_archive(
            jar(&[("META-INF/mods.toml", toml), ("META-INF/MANIFEST.MF", manifest), ("logo.png", &png(16))]),
            Kind::Mod,
        )
        .unwrap();
        assert_eq!(meta.title.as_deref(), Some("Ejemplo Mod"));
        assert_eq!(meta.version.as_deref(), Some("1.2.3"));
        assert_eq!(meta.author.as_deref(), Some("Ana, Luis"));
        assert!(meta.icon.is_some());
        // neoforge.mods.toml tiene prioridad, y sin MANIFEST la versión queda vacía.
        let meta = read_archive(
            jar(&[
                ("META-INF/neoforge.mods.toml", b"[[mods]]\ndisplayName=\"Neo\"\nversion=\"${file.jarVersion}\"\n"),
                ("META-INF/mods.toml", toml),
            ]),
            Kind::Mod,
        )
        .unwrap();
        assert_eq!((meta.title.as_deref(), meta.version), (Some("Neo"), None));
    }

    #[test]
    fn legacy_mcmod_info_is_read() {
        let info = br#"[{"modid":"old","name":"Viejo","version":"1.0","authorList":["Pepe"]}]"#;
        let meta = read_archive(jar(&[("mcmod.info", info)]), Kind::Mod).unwrap();
        assert_eq!((meta.title.as_deref(), meta.author.as_deref()), (Some("Viejo"), Some("Pepe")));
    }

    #[test]
    fn resourcepack_reads_plain_description_and_icon() {
        let mcmeta = serde_json::json!({"pack": {"pack_format": 34, "description": [{"text": "§6Bonito", "extra": [" pack\n"]}, "§r!"]}}).to_string();
        let meta = read_archive(jar(&[("pack.mcmeta", mcmeta.as_bytes()), ("pack.png", &png(128))]), Kind::ResourcePack).unwrap();
        assert_eq!(meta.description.as_deref(), Some("Bonito pack !"));
        assert_eq!(meta.icon.unwrap(), png(128));
        assert!(meta.title.is_none());
        assert_eq!(strip_format("§aVerde §l§nX"), "Verde X");
        // Sin nada reconocible no hay metadatos.
        assert!(read_archive(jar(&[("otra.txt", b"x")]), Kind::ResourcePack).is_none());
    }

    #[test]
    fn oversized_or_lying_entries_are_ignored() {
        let big = format!(r#"{{"name":"{}"}}"#, "a".repeat(200 * 1024));
        // Declarado mayor que 64 KiB.
        assert!(read_archive(archive(&[("fabric.mod.json", big.as_bytes())], CompressionMethod::Deflated), Kind::Mod).is_none());
        assert!(read_archive(jar(&[("fabric.mod.json", big.as_bytes())]), Kind::Mod).is_none());
        // Declarado pequeño (se falsea el tamaño en las dos cabeceras) pero descomprime a mucho más.
        let mut bomb = archive(&[("fabric.mod.json", big.as_bytes())], CompressionMethod::Deflated).into_inner();
        let (real, fake) = ((big.len() as u32).to_le_bytes(), 100u32.to_le_bytes());
        let mut patched = 0;
        for at in 0..bomb.len() - 4 {
            if bomb[at..at + 4] == real {
                bomb[at..at + 4].copy_from_slice(&fake);
                patched += 1;
            }
        }
        assert_eq!(patched, 2);
        assert!(read_archive(Cursor::new(bomb), Kind::Mod).is_none());
        // Icono declarado enorme.
        let mut huge = png(32);
        huge.resize(ICON_BYTES as usize + 1, 0);
        let manifest = br#"{"name":"N","icon":"i.png"}"#;
        let meta = read_archive(jar(&[("fabric.mod.json", manifest), ("i.png", &huge)]), Kind::Mod).unwrap();
        assert!(meta.icon.is_none());
    }

    #[test]
    fn broken_archives_yield_nothing() {
        assert!(read_archive(Cursor::new(b"esto no es un zip".to_vec()), Kind::Mod).is_none());
        let whole = jar(&[("fabric.mod.json", br#"{"name":"N"}"#)]).into_inner();
        assert!(read_archive(Cursor::new(whole[..whole.len() / 2].to_vec()), Kind::Mod).is_none());
        assert!(read_archive(Cursor::new(Vec::new()), Kind::ResourcePack).is_none());
    }

    #[test]
    fn icon_paths_and_images_are_validated() {
        let manifest = |path: &str| format!(r#"{{"name":"N","icon":{}}}"#, serde_json::json!(path)).into_bytes();
        let with_icon = |path: &str, image: &[u8], stored: &str| {
            read_archive(jar(&[("fabric.mod.json", &manifest(path)), (stored, image)]), Kind::Mod).unwrap().icon
        };
        assert!(with_icon("assets/x/icon.png", &png(64), "assets/x/icon.png").is_some());
        assert!(with_icon("./assets/icon.png", &png(64), "assets/icon.png").is_some());
        for path in ["../icon.png", "/icon.png", "a/../icon.png", "a\\icon.png", "C:icon.png", "", "a//b.png"] {
            assert!(with_icon(path, &png(64), "icon.png").is_none(), "{path}");
        }
        assert!(with_icon("i.png", b"GIF89a-no-es-png-pero-largo-como-un-png", "i.png").is_none());
        assert!(with_icon("i.png", &png(513), "i.png").is_none());
        assert!(with_icon("i.png", &png(0), "i.png").is_none());
        assert!(with_icon("i.png", &png(512), "i.png").is_some());
    }

    #[test]
    fn disk_reader_refuses_missing_links_and_huge_files() {
        let dir = std::env::temp_dir().join(format!("openclient-jar-meta-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        assert!(read(&dir.join("falta.jar"), Kind::Mod).is_none());
        let ok = dir.join("ok.jar");
        fs::write(&ok, jar(&[("fabric.mod.json", br#"{"name":"Disco"}"#)]).into_inner()).unwrap();
        assert_eq!(read(&ok, Kind::Mod).unwrap().title.as_deref(), Some("Disco"));
        let huge = dir.join("enorme.jar");
        fs::File::create(&huge).unwrap().set_len(ARCHIVE_LIMIT + 1).unwrap();
        assert!(read(&huge, Kind::Mod).is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&ok, dir.join("enlace.jar")).unwrap();
            assert!(read(&dir.join("enlace.jar"), Kind::Mod).is_none());
        }
        fs::remove_dir_all(dir).unwrap();
    }
}
