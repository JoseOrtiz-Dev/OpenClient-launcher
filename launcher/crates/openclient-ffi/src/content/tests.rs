use super::*;
use std::{
    io::{Cursor, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

const CDN_ICON: &str = "https://cdn.modrinth.com/data/AANobbMI/icon.png";

fn png(side: u32) -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&side.to_be_bytes());
    bytes.extend_from_slice(&side.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bytes
}

fn zip_bytes(entries: &[(&str, &[u8])], method: CompressionMethod) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in entries {
        zip.start_file(*name, SimpleFileOptions::default().compression_method(method)).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn fabric_jar(name: &str, author: &str, with_icon: bool) -> Vec<u8> {
    let manifest = json!({"id": "x", "name": name, "version": "9.9", "authors": [author], "icon": "icon.png"}).to_string();
    let image = png(64);
    let mut entries: Vec<(&str, &[u8])> = vec![("fabric.mod.json", manifest.as_bytes())];
    if with_icon {
        entries.push(("icon.png", &image));
    }
    zip_bytes(&entries, CompressionMethod::Stored)
}

/// Biblioteca temporal con una instancia.
struct Lib {
    root: PathBuf,
    id: String,
}

impl Lib {
    fn new(installed: Value) -> Self {
        let root = std::env::temp_dir().join(format!("openclient-content-{}", uuid::Uuid::new_v4()));
        let id = uuid::Uuid::new_v4().to_string();
        fs::create_dir_all(root.join("instances").join(&id).join("mods")).unwrap();
        let state = json!({"instances": [{"id": id, "name": "P", "mcVersion": "1.21.4", "category": "fabric",
            "installedContent": installed}]});
        fs::write(root.join("launcher_state.json"), state.to_string()).unwrap();
        Self { root, id }
    }

    fn game(&self) -> PathBuf {
        self.root.join("instances").join(&self.id)
    }

    fn put(&self, relative: &str, bytes: &[u8]) {
        let path = self.game().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    fn instance(&self) -> Value {
        let state: Value = serde_json::from_slice(&fs::read(self.root.join("launcher_state.json")).unwrap()).unwrap();
        state["instances"][0].clone()
    }

    fn list(&self, category: Category, now: u64) -> Value {
        list_at(&self.root, &self.instance(), category, now).unwrap()
    }

    fn resolve(&self, category: Category, budget: Duration, now: u64, net: &Net) -> Value {
        resolve_at(&self.root, &self.instance(), category, budget, now, net).unwrap()
    }

    fn request(&self, action: &str, category: &str) -> Value {
        json!({"action": action, "root": self.root, "id": self.id, "category": category})
    }
}

impl Drop for Lib {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn item<'a>(list: &'a Value, key: &str) -> &'a Value {
    list["items"].as_array().unwrap().iter().find(|item| item["key"] == key).unwrap_or_else(|| panic!("falta {key} en {list}"))
}

fn flags(list: &Value, key: &str) -> Vec<String> {
    let mut flags: Vec<String> = item(list, key)["flags"].as_array().unwrap().iter().map(|f| f.as_str().unwrap().to_owned()).collect();
    flags.sort();
    flags
}

/// Servidor HTTP mínimo en 127.0.0.1: responde JSON por ruta y apunta cada petición.
struct Mock {
    api: String,
    hits: Arc<Mutex<Vec<String>>>,
}

fn mock(routes: Vec<(&'static str, Value)>) -> Mock {
    use std::io::Read;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", listener.local_addr().unwrap());
    let hits = Arc::new(Mutex::new(Vec::new()));
    let log = hits.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut data = Vec::new();
            let mut buffer = [0u8; 4096];
            let head_end = loop {
                let read = stream.read(&mut buffer).unwrap_or(0);
                data.extend_from_slice(&buffer[..read]);
                if let Some(at) = data.windows(4).position(|window| window == b"\r\n\r\n") {
                    break at + 4;
                }
                if read == 0 {
                    break data.len();
                }
            };
            let head = String::from_utf8_lossy(&data[..head_end]).to_string();
            let length = head
                .lines()
                .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                .unwrap_or(0);
            while data.len() < head_end + length {
                let read = stream.read(&mut buffer).unwrap_or(0);
                if read == 0 {
                    break;
                }
                data.extend_from_slice(&buffer[..read]);
            }
            let target = head.split_whitespace().nth(1).unwrap_or_default().to_owned();
            let path = target.split('?').next().unwrap_or_default().to_owned();
            log.lock().unwrap().push(path.clone());
            let body = routes.iter().find(|(route, _)| *route == path).map(|(_, body)| body.to_string());
            let response = match body {
                Some(body) => format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                ),
                None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_owned(),
            };
            let _ = stream.write_all(response.as_bytes());
        }
    });
    Mock { api, hits }
}

fn net(api: &str) -> Net {
    Net { client: reqwest::Client::builder().timeout(Duration::from_secs(5)).build().unwrap(), api: api.to_owned() }
}

/// Dirección en la que nadie escucha.
fn dead_api() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

const BUDGET: Duration = Duration::from_secs(30);

fn version_entry(sha1: &str, sha512: &str, size: u64, project: &str) -> Value {
    json!({"id": "V1", "project_id": project, "version_number": "0.6.0",
        "files": [{"hashes": {"sha1": sha1, "sha512": sha512}, "size": size}]})
}

#[test]
fn list_merges_installed_manifest_and_disk() {
    let sha = |c: char| c.to_string().repeat(40);
    let lib = Lib::new(json!([
        {"id": "AANobbMI", "versionId": "v1", "type": "mod", "source": "modrinth", "title": "Sodium", "version": "0.6.0", "enabled": true},
        {"id": "gvQqBUqZ", "versionId": "v2", "type": "mod", "source": "modrinth", "title": "Lithium", "version": "0.14", "enabled": false},
        {"id": "YL57xq9U", "versionId": "auto", "type": "mod", "source": "modrinth", "title": "Iris", "version": "Auto", "enabled": true},
        {"id": "cf99", "versionId": "1", "type": "mod", "source": "curseforge", "title": "CF Mod", "fileName": "../sub/cf-mod.jar", "enabled": true},
        {"id": "pack1", "versionId": "p1", "type": "resourcepack", "source": "modrinth", "title": "Otro tipo", "enabled": true}
    ]));
    let cache = json!({
        "version:v1": {"id": "v1", "project_id": "AANobbMI", "version_number": "0.6.0",
            "files": [{"filename": "sodium-0.6.jar", "primary": true, "hashes": {"sha1": sha('a')}}]},
        "version:v2": {"id": "v2", "project_id": "gvQqBUqZ", "version_number": "0.14",
            "files": [{"filename": "../lithium-0.14.jar", "hashes": {"sha1": sha('b')}}]},
        "version:d1": {"id": "d1", "project_id": "P5fabric", "version_number": "1.0",
            "files": [{"filename": "fabric-api.jar", "primary": true, "hashes": {"sha1": sha('c')}}]},
    });
    lib.put("mods/.openclient_versions.json", cache.to_string().as_bytes());
    lib.put("mods/.openclient_managed.json", br#"["sodium-0.6.jar","fabric-api.jar","clash.jar"]"#);
    for name in [
        "sodium-0.6.jar", "sodium-0.6.jar.disabled", "fabric-api.jar", "lithium-0.14.jar.disabled", "cf-mod.jar",
        "local.jar", "off.jar.disabled", "X.JAR", "dup.jar", "dup.jar.disabled", "clash.jar.disabled",
        "readme.txt", ".hidden.jar", "otro.jar.DISABLED",
    ] {
        lib.put(&format!("mods/{name}"), b"x");
    }
    fs::create_dir(lib.game().join("mods/carpeta.jar")).unwrap();

    let list = lib.list(Category::Mods, 1000);
    assert_eq!((list["category"].as_str(), list["pending"].as_u64(), list["offline"].as_bool()), (Some("mods"), Some(11), Some(false)));
    assert_eq!(list["truncated"], false);
    assert_eq!(list["items"].as_array().unwrap().len(), 13, "{list}");

    let sodium = item(&list, "p:AANobbMI");
    assert_eq!((sodium["managed"].as_bool(), sodium["present"].as_bool(), sodium["enabled"].as_bool()), (Some(true), Some(true), Some(true)));
    assert_eq!((sodium["fileName"].as_str(), sodium["title"].as_str(), sodium["version"].as_str()), (Some("sodium-0.6.jar"), Some("Sodium"), Some("0.6.0")));
    assert_eq!((sodium["kind"].as_str(), sodium["source"].as_str(), sodium["dependency"].as_bool()), (Some("mod"), Some("modrinth"), Some(false)));
    assert_eq!(sodium["projectId"], "AANobbMI");
    assert!(sodium["updateAvailable"].is_null());
    assert_eq!(flags(&list, "p:AANobbMI"), ["duplicate"]);
    // x.jar y x.jar.disabled a la vez: las dos filas llevan `duplicate`; la desactivada además choca con el manifiesto.
    let twin = item(&list, "f:sodium-0.6.jar.disabled");
    assert_eq!((twin["managed"].as_bool(), twin["enabled"].as_bool()), (Some(false), Some(false)));
    assert_eq!(flags(&list, "f:sodium-0.6.jar.disabled"), ["duplicate", "nameClash"]);
    assert_eq!(flags(&list, "f:dup.jar"), ["duplicate"]);
    assert_eq!(flags(&list, "f:dup.jar.disabled"), ["duplicate"]);

    // Gestionado desactivado: sync lo borró; el nombre sale de la caché (sin `../`). El
    // `.disabled` suelto con ese nombre no es la fila gestionada y choca con ella.
    let lithium = item(&list, "p:gvQqBUqZ");
    assert_eq!((lithium["present"].as_bool(), lithium["enabled"].as_bool(), lithium["fileName"].as_str()), (Some(false), Some(false), Some("lithium-0.14.jar")));
    assert_eq!((item(&list, "f:lithium-0.14.jar")["managed"].as_bool(), flags(&list, "f:lithium-0.14.jar")), (Some(false), vec!["nameClash".to_owned()]));
    // Gestionado activado pero aún sin descargar.
    let iris = item(&list, "p:YL57xq9U");
    assert_eq!((iris["present"].as_bool(), iris["enabled"].as_bool(), iris["fileName"].as_str()), (Some(false), Some(true), Some("")));
    // CurseForge: el nombre viene del estado y se reduce a su último componente.
    let cf = item(&list, "p:cf99");
    assert_eq!((cf["present"].as_bool(), cf["fileName"].as_str(), cf["source"].as_str()), (Some(true), Some("cf-mod.jar"), Some("file")));
    // Dependencia: en el manifiesto pero no elegida por el usuario.
    let dependency = item(&list, "p:P5fabric");
    assert_eq!((dependency["managed"].as_bool(), dependency["dependency"].as_bool(), dependency["version"].as_str()), (Some(true), Some(true), Some("1.0")));
    assert_eq!(dependency["fileName"], "fabric-api.jar");
    // Sueltos.
    let local = item(&list, "f:local.jar");
    assert_eq!((local["managed"].as_bool(), local["enabled"].as_bool(), local["title"].as_str()), (Some(false), Some(true), Some("local")));
    assert_eq!(local["size"], 1);
    let off = item(&list, "f:off.jar");
    assert_eq!((off["enabled"].as_bool(), off["fileName"].as_str(), off["title"].as_str()), (Some(false), Some("off.jar.disabled"), Some("off")));
    assert_eq!(item(&list, "f:x.jar")["fileName"], "X.JAR");
    assert_eq!(flags(&list, "f:clash.jar"), ["nameClash"]);
    let keys: Vec<&str> = list["items"].as_array().unwrap().iter().map(|i| i["key"].as_str().unwrap()).collect();
    assert!(keys.iter().all(|key| !key.contains("readme") && !key.contains("hidden") && !key.contains("carpeta") && !key.contains("otro")));
}

#[test]
fn list_covers_packs_with_the_sync_manifest_and_names() {
    let lib = Lib::new(json!([
        {"id": "PackAaa1", "versionId": "Ver1", "type": "resourcepack", "source": "modrinth", "title": "Bonito", "version": "1.2", "enabled": true},
        {"id": "ShadBbb2", "versionId": "Ver2", "type": "shader", "source": "modrinth", "title": "Shaders", "enabled": true}
    ]));
    lib.put("resourcepacks/OpenClient-PackAaa1-Ver1.zip", b"zip");
    lib.put("resourcepacks/Mio.zip", b"zip");
    lib.put("resourcepacks/.openclient-packs.json", json!({"OpenClient-PackAaa1-Ver1.zip": {"url": "https://cdn.modrinth.com/x", "sha1": "a".repeat(40), "size": 3}}).to_string().as_bytes());
    lib.put("shaderpacks/Sombras Brillantes.zip.disabled", b"zip");
    lib.put("shaderpacks/solo.jar", b"no es un shader");
    let packs = lib.list(Category::Resourcepacks, 1);
    assert_eq!(packs["items"].as_array().unwrap().len(), 2);
    let managed = item(&packs, "p:PackAaa1");
    assert_eq!((managed["kind"].as_str(), managed["present"].as_bool(), managed["fileName"].as_str()), (Some("resourcepack"), Some(true), Some("OpenClient-PackAaa1-Ver1.zip")));
    assert_eq!(item(&packs, "f:mio.zip")["title"], "Mio");
    let shaders = lib.list(Category::Shaderpacks, 1);
    assert_eq!(shaders["items"].as_array().unwrap().len(), 2);
    assert_eq!(item(&shaders, "f:sombras brillantes.zip")["enabled"], false);
    assert_eq!(item(&shaders, "f:sombras brillantes.zip")["title"], "Sombras Brillantes");
    assert_eq!(item(&shaders, "p:ShadBbb2")["present"], false);
    assert_eq!(lib.list(Category::Shaderpacks, 1)["pending"], 1);
    // Una carpeta que no existe se lista vacía.
    assert_eq!(lib.list(Category::Shaderpacks, 1)["items"].as_array().unwrap().len(), 2);
    fs::remove_dir_all(lib.game().join("resourcepacks")).unwrap();
    assert_eq!(lib.list(Category::Resourcepacks, 1)["pending"], 0);
}

#[test]
fn requests_are_validated_before_touching_the_disk() {
    let lib = Lib::new(json!([]));
    let error = |request: Value| crate::launcher_operation(&request).unwrap_err();
    for category in ["../x", "Mods", "", "mods/../..", "worlds"] {
        assert_eq!(error(lib.request("content_list", category)), "Categoría de contenido inválida.");
        assert_eq!(error(lib.request("content_resolve", category)), "Categoría de contenido inválida.");
    }
    let mut bad_id = lib.request("content_list", "mods");
    for id in ["../x", "no-es-uuid", "", "{00000000-0000-0000-0000-000000000000}"] {
        bad_id["id"] = json!(id);
        assert!(crate::launcher_operation(&bad_id).is_err(), "{id}");
    }
    assert!(instance_dir(&lib.root, "urn:uuid:00000000-0000-0000-0000-000000000000").is_err());
    assert!(instance_dir(&lib.root, "../instances").is_err());
    // Dispatch real de `content_list`.
    lib.put("mods/a.jar", b"x");
    let listed = crate::launcher_operation(&lib.request("content_list", "mods")).unwrap();
    assert_eq!((listed["pending"].as_u64(), listed["items"].as_array().unwrap().len()), (Some(1), 1));
}

/// Enlace de directorio: symlink en Unix; junction (sin privilegios) en Windows.
fn link_dir(link: &Path, target: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link).is_ok()
    }
    #[cfg(windows)]
    {
        std::process::Command::new(std::env::var_os("ComSpec").unwrap_or_else(|| "cmd".into()))
            .args(["/C", "mklink", "/J"])
            // `mklink` toma `/` como inicio de una opción: rutas solo con `\`.
            .arg(link.to_string_lossy().replace('/', "\\"))
            .arg(target)
            .output()
            .is_ok_and(|output| output.status.success())
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (link, target);
        false
    }
}

#[test]
fn linked_folders_and_entries_are_neither_listed_nor_followed() {
    let lib = Lib::new(json!([]));
    let outside = std::env::temp_dir().join(format!("openclient-outside-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("secreto.jar"), b"x").unwrap();
    // Enlace dentro de `mods/`: ni se lista ni se sigue.
    lib.put("mods/real.jar", b"x");
    let supported = cfg!(any(unix, windows));
    assert_eq!(link_dir(&lib.game().join("mods/enlace.jar"), &outside), supported);
    if supported {
        let list = lib.list(Category::Mods, 1);
        assert_eq!(list["items"].as_array().unwrap().len(), 1);
    }
    // `resourcepacks` entero como enlace: error.
    if supported && link_dir(&lib.game().join("resourcepacks"), &outside) {
        assert!(list_at(&lib.root, &lib.instance(), Category::Resourcepacks, 1).is_err());
        assert!(resolve_at(&lib.root, &lib.instance(), Category::Resourcepacks, BUDGET, 1, &net(&dead_api())).is_err());
    }
    // La carpeta de la instancia como enlace: error.
    let other = Lib::new(json!([]));
    fs::remove_dir_all(other.game()).unwrap();
    if supported && link_dir(&other.game(), &outside) {
        assert!(list_at(&other.root, &other.instance(), Category::Mods, 1).is_err());
    }
    // El destino del enlace no se tocó.
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    // Un destino de enlace se quita sin borrar su contenido.
    let _ = fs::remove_dir(lib.game().join("mods/enlace.jar"));
    let _ = fs::remove_dir(lib.game().join("resourcepacks"));
    let _ = fs::remove_dir(other.game());
    fs::remove_dir_all(outside).unwrap();
}

#[test]
fn hostile_names_from_state_and_cache_never_become_paths() {
    let lib = Lib::new(json!([
        {"id": "AANobbMI", "versionId": "v1", "type": "mod", "title": "Sodium", "enabled": true},
        {"id": "Evil1234", "versionId": "v9", "type": "mod", "source": "curseforge", "fileName": "..\\..\\evil.jar", "enabled": true},
        {"id": "../x", "versionId": "v1", "type": "mod", "title": "Id roto", "enabled": true}
    ]));
    lib.put("mods/.openclient_versions.json", json!({
        "version:v1": {"id": "v1", "project_id": "AANobbMI", "files": [{"filename": "../fuera.jar", "primary": true}]}
    }).to_string().as_bytes());
    lib.put("mods/.openclient_managed.json", br#"["../fuera.jar","a/b.jar","C:x.jar"]"#);
    lib.put("mods/fuera.jar", b"x");
    let list = lib.list(Category::Mods, 1);
    // `../fuera.jar` se reduce a `fuera.jar` (como hace sync), que sí está en `mods/`.
    assert_eq!(item(&list, "p:AANobbMI")["fileName"], "fuera.jar");
    assert_eq!(list["items"].as_array().unwrap().len(), 2, "{list}");
    assert!(!list.to_string().contains("Id roto"));
    assert!(!lib.root.join("instances/fuera.jar").exists() && !lib.game().join("fuera.jar").exists());
}

#[test]
fn version_files_response_is_validated_like_the_export() {
    let entry = |sha1: &str, sha512: &str, size: u64| Entry {
        size,
        sha1: sha1.into(),
        sha512: sha512.into(),
        origin: "jar".into(),
        title: "Del jar".into(),
        ..Entry::default()
    };
    let (good, bad_sha1, bad_size, absent) = (
        entry(&"a".repeat(40), &"1".repeat(128), 10),
        entry(&"b".repeat(40), &"2".repeat(128), 10),
        entry(&"c".repeat(40), &"3".repeat(128), 10),
        entry(&"d".repeat(40), &"4".repeat(128), 10),
    );
    let response = json!({
        "1".repeat(128): version_entry(&good.sha1, &good.sha512, 10, "AANobbMI"),
        "2".repeat(128): version_entry(&"0".repeat(40), &bad_sha1.sha512, 10, "AANobbMI"),
        "3".repeat(128): version_entry(&bad_size.sha1, &bad_size.sha512, 11, "AANobbMI"),
    });
    let mut entries = [good, bad_sha1, bad_size, absent];
    apply_version_files(entries.iter_mut(), &response, 500);
    assert_eq!((entries[0].project_id.as_str(), entries[0].version_id.as_str(), entries[0].version.as_str()), ("AANobbMI", "V1", "0.6.0"));
    assert_eq!((entries[0].origin.as_str(), entries[0].not_found, entries[0].resolved_at), ("modrinth", false, 0));
    for rejected in &entries[1..] {
        assert!(rejected.not_found && rejected.resolved_at == 500 && rejected.project_id.is_empty(), "{rejected:?}");
        assert_eq!(rejected.origin, "jar");
    }
    // Una respuesta que no es un objeto no cambia nada.
    let mut untouched = [entry(&"a".repeat(40), &"1".repeat(128), 10)];
    apply_version_files(untouched.iter_mut(), &json!([]), 7);
    assert!(!untouched[0].not_found && untouched[0].resolved_at == 0);

    // Proyectos: el icono fuera del CDN se descarta; el equipo aporta el autor.
    let infos = content_meta::project_infos(&json!([
        {"id": "AANobbMI", "title": "Sodium", "icon_url": CDN_ICON, "team": "TEAM1"},
        {"id": "OtroProj1", "title": "Otro", "icon_url": "https://evil.example/data/x.png", "team": "TEAM2"},
    ]));
    let mut sodium = Entry { project_id: "AANobbMI".into(), icon: "cache:".to_owned() + &"e".repeat(40), ..Entry::default() };
    let mut other = Entry { project_id: "OtroProj1".into(), icon: "cache:keep".into(), ..Entry::default() };
    apply_projects([&mut sodium, &mut other], &infos);
    assert_eq!((sodium.title.as_str(), sodium.icon.as_str()), ("Sodium", CDN_ICON));
    assert_eq!((other.title.as_str(), other.icon.as_str()), ("Otro", "cache:keep"));
    let authors = content_meta::team_authors(&json!([[
        {"team_id": "TEAM1", "role": "Member", "ordering": 0, "user": {"username": "miembro"}},
        {"team_id": "TEAM1", "role": "Owner", "ordering": 3, "user": {"username": "jellysquid3"}}
    ]]));
    finish_projects([&mut sodium, &mut other], &infos, &authors, 900);
    assert_eq!((sodium.author.as_str(), sodium.resolved_at), ("jellysquid3", 900));
    assert_eq!((other.author.as_str(), other.resolved_at), ("", 900));
}

#[test]
fn resolve_identifies_files_reads_jars_and_caches_everything() {
    let lib = Lib::new(json!([]));
    let sodium = fabric_jar("Sodium (jar)", "autor-del-jar", true);
    let bomb = format!(r#"{{"name":"{}"}}"#, "a".repeat(200 * 1024));
    lib.put("mods/sodium.jar", &sodium);
    lib.put("mods/copia.jar", &sodium);
    lib.put("mods/local.jar", &fabric_jar("Mod Local", "yo", true));
    lib.put("mods/bomba.jar", &zip_bytes(&[("fabric.mod.json", bomb.as_bytes())], CompressionMethod::Deflated));
    lib.put("mods/falso.jar", b"esto no es un zip");
    lib.put("mods/sodium.jar.disabled.bak", b"no listado");
    let hash = |name: &str| content_meta::hash_file(&lib.game().join("mods").join(name)).unwrap();
    let (sha1, sha512, size) = hash("sodium.jar");
    let (local1, local512, local_size) = hash("local.jar");
    let server = mock(vec![
        ("/version_files", json!({
            sha512.clone(): version_entry(&sha1, &sha512, size, "AANobbMI"),
            local512.clone(): version_entry(&"0".repeat(40), &local512, local_size, "AANobbMI"),
        })),
        ("/projects", json!([{"id": "AANobbMI", "title": "Sodium", "icon_url": CDN_ICON, "team": "TEAM1"}])),
        ("/teams", json!([[{"team_id": "TEAM1", "role": "Owner", "ordering": 0, "user": {"username": "jellysquid3"}}]])),
    ]);
    let before = lib.list(Category::Mods, 1000);
    assert_eq!(before["pending"], 5);
    assert_eq!(item(&before, "f:sodium.jar")["source"], "file");

    let resolved = lib.resolve(Category::Mods, BUDGET, 1000, &net(&server.api));
    assert_eq!((resolved["pending"].as_u64(), resolved["offline"].as_bool()), (Some(0), Some(false)));
    assert_eq!(*server.hits.lock().unwrap(), ["/version_files", "/projects", "/teams"]);
    let list = lib.list(Category::Mods, 1000);
    assert_eq!((list["pending"].as_u64(), list["offline"].as_bool()), (Some(0), Some(false)));
    // Solo cambian las filas con datos nuevos: las dos de Modrinth y la del jar.
    assert_eq!(resolved["items"].as_array().unwrap().len(), 3);
    for key in ["f:sodium.jar", "f:copia.jar"] {
        let row = item(&list, key);
        assert_eq!((row["title"].as_str(), row["author"].as_str(), row["version"].as_str()), (Some("Sodium"), Some("jellysquid3"), Some("0.6.0")), "{row}");
        assert_eq!((row["icon"].as_str(), row["source"].as_str(), row["projectId"].as_str()), (Some(CDN_ICON), Some("modrinth"), Some("AANobbMI")));
        assert_eq!(row["versionId"], "V1");
        // Mismo proyecto en dos archivos sueltos.
        assert_eq!(flags(&list, key), ["duplicate"]);
    }
    // Hash discordante: Modrinth no lo identifica y se usan los metadatos del jar y su icono.
    let local = item(&list, "f:local.jar");
    assert_eq!((local["title"].as_str(), local["author"].as_str(), local["version"].as_str(), local["source"].as_str()), (Some("Mod Local"), Some("yo"), Some("9.9"), Some("jar")));
    assert_eq!(local["icon"], format!("cache:{local1}"));
    assert_eq!(fs::read(lib.root.join("cache/content-icons").join(format!("{local1}.png"))).unwrap(), png(64));
    assert!(flags(&list, "f:local.jar").is_empty());
    // Jar con entrada gigante y archivo que no es zip: solo el nombre, sin error.
    for (key, title) in [("f:bomba.jar", "bomba"), ("f:falso.jar", "falso")] {
        let row = item(&list, key);
        assert_eq!((row["source"].as_str(), row["title"].as_str(), row["icon"].as_str()), (Some("file"), Some(title), Some("")));
    }
    // Índice persistido, con `notFound` para lo que Modrinth rechazó, y sin temporales.
    let index = content_index::load(&lib.game());
    assert_eq!(index.entries.len(), 5);
    assert!(index.entries["mods/local.jar"].not_found && !index.entries["mods/sodium.jar"].not_found);
    assert_eq!(index.entries["mods/sodium.jar"].sha512, sha512);
    assert!(fs::read_dir(lib.game()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains(".tmp-")));
    // Una segunda llamada no cambia nada ni sale a la red.
    let again = lib.resolve(Category::Mods, BUDGET, 1001, &net(&server.api));
    assert_eq!((again["items"].as_array().unwrap().len(), again["pending"].as_u64()), (0, Some(0)));
    assert_eq!(server.hits.lock().unwrap().len(), 3);
    // Desactivar un archivo (renombrar) conserva su entrada: no hay nada pendiente.
    fs::rename(lib.game().join("mods/sodium.jar"), lib.game().join("mods/sodium.jar.disabled")).unwrap();
    let toggled = lib.list(Category::Mods, 1002);
    assert_eq!(toggled["pending"], 0);
    assert_eq!((item(&toggled, "f:sodium.jar")["enabled"].as_bool(), item(&toggled, "f:sodium.jar")["title"].as_str()), (Some(false), Some("Sodium")));
    // Un archivo modificado vuelve a ser pendiente.
    lib.put("mods/falso.jar", b"ahora es otro contenido distinto");
    assert_eq!(lib.list(Category::Mods, 1003)["pending"], 1);
}

#[test]
fn managed_rows_prefer_modrinth_flag_local_edits_and_dedupe_by_project() {
    let lib = Lib::new(json!([
        {"id": "AANobbMI", "versionId": "v1", "type": "mod", "source": "modrinth", "title": "Sodium (estado)", "version": "estado", "enabled": true}
    ]));
    lib.put("mods/sodium-0.6.jar", &fabric_jar("Sodium (jar)", "yo", false));
    lib.put("mods/otro-sodium.jar", &fabric_jar("Sodium (jar)", "yo", false));
    let (sha1, sha512, size) = content_meta::hash_file(&lib.game().join("mods/sodium-0.6.jar")).unwrap();
    lib.put("mods/.openclient_versions.json", json!({
        "version:v1": {"id": "v1", "project_id": "AANobbMI", "files": [{"filename": "sodium-0.6.jar", "primary": true,
            "hashes": {"sha1": "9".repeat(40)}}]}
    }).to_string().as_bytes());
    let server = mock(vec![
        ("/version_files", json!({sha512.clone(): version_entry(&sha1, &sha512, size, "AANobbMI")})),
        ("/projects", json!([{"id": "AANobbMI", "title": "Sodium", "team": "TEAM1"}])),
        ("/teams", json!([])),
    ]);
    lib.resolve(Category::Mods, BUDGET, 10, &net(&server.api));
    let list = lib.list(Category::Mods, 10);
    let managed = item(&list, "p:AANobbMI");
    // El hash gana al estado (sin equipo en Modrinth, el autor sale del jar), y el sha1 distinto del registrado marca la edición local.
    assert_eq!((managed["title"].as_str(), managed["version"].as_str(), managed["author"].as_str()), (Some("Sodium"), Some("0.6.0"), Some("yo")));
    assert_eq!(flags(&list, "p:AANobbMI"), ["modifiedLocally"]);
    // El suelto con el mismo contenido (mismo proyecto por hash) se marca como duplicado.
    assert_eq!(flags(&list, "f:otro-sodium.jar"), ["duplicate"]);
}

#[test]
fn network_failures_keep_local_data_and_never_mark_not_found() {
    let lib = Lib::new(json!([]));
    lib.put("mods/uno.jar", &fabric_jar("Uno", "a", true));
    lib.put("mods/dos.jar", &fabric_jar("Dos", "b", false));
    let dead = net(&dead_api());
    let first = lib.resolve(Category::Mods, BUDGET, 5000, &dead);
    assert_eq!((first["offline"].as_bool(), first["pending"].as_u64()), (Some(true), Some(0)));
    let list = lib.list(Category::Mods, 5010);
    assert_eq!((list["offline"].as_bool(), list["pending"].as_u64()), (Some(true), Some(0)));
    let uno = item(&list, "f:uno.jar");
    assert_eq!((uno["title"].as_str(), uno["source"].as_str(), uno["author"].as_str()), (Some("Uno"), Some("jar"), Some("a")));
    assert!(uno["icon"].as_str().unwrap().starts_with("cache:"));
    let index = content_index::load(&lib.game());
    assert_eq!(index.offline_at, 5000);
    assert!(index.entries.values().all(|entry| !entry.not_found && entry.resolved_at == 0));
    // Pasado el TTL sin conexión se vuelve a pedir la red.
    assert_eq!(lib.list(Category::Mods, 5000 + OFFLINE_TTL + 1)["pending"], 2);
    // Con conexión, y ya sin entrada de red pendiente, se limpia el estado sin conexión.
    let (sha1, sha512, size) = content_meta::hash_file(&lib.game().join("mods/uno.jar")).unwrap();
    let server = mock(vec![
        ("/version_files", json!({sha512.clone(): version_entry(&sha1, &sha512, size, "AANobbMI")})),
        ("/projects", json!([{"id": "AANobbMI", "title": "Uno Modrinth", "icon_url": CDN_ICON, "team": "T9"}])),
        ("/teams", json!([[{"team_id": "T9", "user": {"username": "primero"}, "ordering": 4}]])),
    ]);
    let later = lib.resolve(Category::Mods, BUDGET, 6000, &net(&server.api));
    assert_eq!((later["offline"].as_bool(), later["pending"].as_u64()), (Some(false), Some(0)));
    let list = lib.list(Category::Mods, 6000);
    assert_eq!((item(&list, "f:uno.jar")["title"].as_str(), item(&list, "f:uno.jar")["author"].as_str()), (Some("Uno Modrinth"), Some("primero")));
    assert_eq!(item(&list, "f:uno.jar")["icon"], CDN_ICON);
    assert!(content_index::load(&lib.game()).entries["mods/dos.jar"].not_found);
    // Un `notFound` caducado se reintenta, pero si la red falla se conserva intacto.
    let week = 8 * 24 * 3600;
    assert_eq!(lib.list(Category::Mods, 6000 + week)["pending"], 1);
    let failed = lib.resolve(Category::Mods, BUDGET, 6000 + week, &dead);
    assert_eq!(failed["offline"], true);
    let kept = &content_index::load(&lib.game()).entries["mods/dos.jar"];
    assert!(kept.not_found && kept.resolved_at == 6000);
}

#[test]
fn resolve_hashes_at_least_one_file_per_call_within_the_budget() {
    let lib = Lib::new(json!([]));
    for name in ["a", "b", "c"] {
        lib.put(&format!("mods/{name}.jar"), &fabric_jar(name, "x", false));
    }
    let dead = net(&dead_api());
    let mut pending = vec![];
    for step in 0..3 {
        let response = lib.resolve(Category::Mods, Duration::ZERO, 100 + step, &dead);
        assert_eq!(response["items"].as_array().unwrap().len(), 1, "{response}");
        pending.push(response["pending"].as_u64().unwrap());
    }
    assert_eq!(pending, [2, 1, 0]);
}

#[test]
fn a_corrupt_index_is_rebuilt_without_errors() {
    let lib = Lib::new(json!([]));
    lib.put("mods/a.jar", &fabric_jar("A", "x", false));
    lib.put(content_index::FILE, b"{ esto esta roto");
    let list = lib.list(Category::Mods, 1);
    assert_eq!((list["pending"].as_u64(), list["items"].as_array().unwrap().len()), (Some(1), 1));
    lib.resolve(Category::Mods, BUDGET, 2, &net(&dead_api()));
    assert!(content_index::load(&lib.game()).entries.contains_key("mods/a.jar"));
    assert_eq!(lib.list(Category::Mods, 3)["pending"], 0);
}
