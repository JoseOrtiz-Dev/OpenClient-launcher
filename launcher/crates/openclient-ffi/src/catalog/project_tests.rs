use super::*;

#[test]
fn curseforge_catalog_routes_stop_before_reaching_network_or_library() {
    for (handler, request) in [
        (
            search as fn(&Value) -> Result<Value, String>,
            json!({"action": "catalog_discover", "provider": "curseforge", "kind": "mod"}),
        ),
        (
            project,
            json!({"action": "catalog_project", "project": "cf12345", "kind": "mod"}),
        ),
        (
            change,
            json!({"action": "content_add", "project": "cf12345", "kind": "mod"}),
        ),
        (
            install_pack,
            json!({"action": "modpack_install", "project": "cf12345"}),
        ),
    ] {
        assert_eq!(
            handler(&request).unwrap_err(),
            "CurseForge estará disponible próximamente."
        );
    }
}

#[test]
fn existing_curseforge_content_can_be_disabled_and_removed() {
    let id = uuid::Uuid::new_v4();
    let root = std::env::temp_dir().join(format!("openclient-legacy-catalog-{id}"));
    std::fs::create_dir(&root).unwrap();
    let state_path = root.join("launcher_state.json");
    let project = "cf12345";
    write_state_locked(
        &state_path,
        &json!({"instances": [{
            "id": id.to_string(), "mcVersion": "1.21.4", "category": "fabric",
            "installedContent": [{"id": project, "source":"curseforge", "type":"mod", "enabled":true}]
        }]})
        .to_string(),
    )
    .unwrap();
    let mut request = json!({
        "action": "content_toggle", "id": id.to_string(), "root": root, "project": project
    });
    let disabled = change(&request).unwrap();
    request["action"] = json!("content_remove");
    let removed = change(&request).unwrap();
    let persistent: Value =
        serde_json::from_str(&load_state_at(&state_path).unwrap().unwrap()).unwrap();
    std::fs::remove_dir_all(root).unwrap();

    assert_eq!(disabled[0]["installedContent"][0]["enabled"], false);
    assert_eq!(removed[0]["installedContent"], json!([]));
    assert_eq!(persistent["instances"][0]["installedContent"], json!([]));
}

fn version(project: &str, loader: &str, game: &str) -> Value {
    json!({"id":"version1", "project_id":project, "loaders":[loader],
        "game_versions":[game], "files":[{"filename":"example.jar"}],
        "version_number":"1.0", "version_type":"release"})
}

#[test]
fn explicit_selection_never_falls_back_to_a_different_version() {
    let stable = version("project1", "fabric", "1.21.4");
    let mut beta = stable.clone();
    beta["id"] = json!("beta1");
    beta["version_type"] = json!("beta");
    let versions = vec![beta, stable];
    assert_eq!(
        pick_version(&versions, "project1", "mod", Some("1.21.4"), &Value::Null).unwrap()["id"],
        "version1"
    );
    assert_eq!(
        pick_version(
            &versions,
            "project1",
            "mod",
            Some("1.21.4"),
            &json!("beta1")
        )
        .unwrap()["id"],
        "beta1"
    );
    for invalid in [json!("missing"), json!("../version1"), json!(false)] {
        assert!(pick_version(&versions, "project1", "mod", Some("1.21.4"), &invalid).is_err());
    }
    assert!(pick_version(
        &versions,
        "project1",
        "mod",
        Some("1.20.1"),
        &json!("version1")
    )
    .is_err());
    assert!(pick_version(
        &versions,
        "other",
        "mod",
        Some("1.21.4"),
        &json!("version1")
    )
    .is_err());
}

#[test]
fn dependency_conflicts_respect_pins_and_disabled_projects() {
    let project_wide = json!({"project_id":"dep1", "dependency_type":"incompatible"});
    let version_only = json!({"version_id":"bad1", "dependency_type":"incompatible"});
    let installed = json!([{"id":"dep1", "versionId":"good1", "enabled":true}]);
    assert!(dependency_conflict(&project_wide, &installed));
    assert!(!dependency_conflict(&version_only, &installed));
    assert!(dependency_conflict(
        &version_only,
        &json!([{"id":"dep1","versionId":"bad1","enabled":true}])
    ));
    assert!(!dependency_conflict(
        &project_wide,
        &json!([{"id":"dep1","enabled":false}])
    ));
    assert!(!dependency_conflict(
        &json!({"project_id":"dep1","dependency_type":"required"}),
        &installed
    ));
}

#[test]
fn version_details_include_dependency_types_without_losing_exact_pins() {
    let mut item = version("project1", "fabric", "1.21.4");
    item["dependencies"] = json!([{"project_id":"dep1", "version_id":"depversion1", "dependency_type":"required"},
        {"project_id":"dep2", "dependency_type":"optional"}, {"file_name":"bundled.jar", "dependency_type":"embedded"}]);
    let details = project_summary(
        "project1",
        "mod",
        Some("1.21.4"),
        &json!({"id":"project1","project_type":"mod"}),
        &json!([item]),
    )
    .unwrap();
    assert_eq!(
        details["versions"][0]["dependencies"][0]["version_id"],
        "depversion1"
    );
    assert_eq!(
        details["versions"][0]["dependencies"][2]["dependency_type"],
        "embedded"
    );
}

#[test]
fn modpack_selection_accepts_the_four_loaders_and_requires_a_pack_file() {
    let jar = version("pack1", "fabric", "1.21.4");
    let mut pack = jar.clone();
    pack["id"] = json!("packversion1");
    pack["files"] = json!([{"filename":"example.MRPACK"}]);
    let mut versions = vec![jar, pack.clone()];
    for loader in ["quilt", "forge", "neoforge"] {
        let mut other = pack.clone();
        other["id"] = json!(format!("{loader}1"));
        other["loaders"] = json!([loader]);
        versions.push(other);
    }
    let mut unknown = pack;
    unknown["id"] = json!("rift1");
    unknown["loaders"] = json!(["rift"]);
    versions.push(unknown);
    for id in ["packversion1", "quilt1", "forge1", "neoforge1"] {
        assert_eq!(
            pick_version(&versions, "pack1", "modpack", None, &json!(id)).unwrap()["id"],
            id
        );
    }
    assert!(pick_version(&versions, "pack1", "modpack", None, &json!("rift1")).is_err());
    assert!(pick_version(&versions, "pack1", "modpack", None, &json!("version1")).is_err());
}

#[test]
fn project_details_reject_unrelated_versions_and_untrusted_images() {
    let metadata = json!({"id":"project1", "project_type":"mod", "title":"Example",
        "gallery":[{"url":"https://cdn.modrinth.com/data/project1/images/a.png"},
                   {"url":"http://127.0.0.1/private"},
                   {"url":"https://cdn.modrinth.com.evil.test/data/image.png"}]});
    let versions = json!([
        version("project1", "fabric", "1.21.4"),
        version("other", "fabric", "1.21.4"),
        version("project1", "forge", "1.21.4"),
        version("project1", "fabric", "1.20.1")
    ]);
    let details = project_summary("project1", "mod", Some("1.21.4"), &metadata, &versions).unwrap();
    assert_eq!(details["versions"].as_array().unwrap().len(), 1);
    assert_eq!(details["gallery"].as_array().unwrap().len(), 1);
    assert!(project_summary("other", "mod", None, &metadata, &versions).is_err());
    assert!(project_summary("project1", "shader", None, &metadata, &versions).is_err());
}

#[test]
fn project_details_allow_empty_versions_but_reject_malformed_responses() {
    let metadata = json!({"id":"pack1", "project_type":"modpack"});
    let versions = json!([version("pack1", "forge", "1.21.4")]);
    let details = project_summary("pack1", "modpack", None, &metadata, &versions).unwrap();
    assert_eq!(details["versions"], json!([]));
    assert!(project_summary("pack1", "modpack", None, &metadata, &json!({})).is_err());
}

#[test]
fn project_details_prefer_stable_and_use_iris_for_shaders() {
    let metadata = json!({"id":"shader1", "project_type":"shader"});
    let mut beta = version("shader1", "iris", "1.21.4");
    beta["version_type"] = json!("beta");
    beta["id"] = json!("beta1");
    let versions = json!([
        beta,
        version("shader1", "iris", "1.21.4"),
        version("shader1", "optifine", "1.21.4")
    ]);
    let details =
        project_summary("shader1", "shader", Some("1.21.4"), &metadata, &versions).unwrap();
    assert_eq!(details["versions"].as_array().unwrap().len(), 2);
    assert_eq!(details["recommended"]["id"], "version1");
}

fn library_with(content: Value) -> (std::path::PathBuf, String, String) {
    let id = uuid::Uuid::new_v4().to_string();
    let root = std::env::temp_dir().join(format!("openclient-update-content-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let state = json!({"instances": [{
        "id": id, "name": "n", "mcVersion": "1.21.4", "category": "vanilla", "installedContent": content
    }]})
    .to_string();
    write_state_locked(&root.join("launcher_state.json"), &state).unwrap();
    (root, id, state)
}

#[test]
fn content_update_skips_every_origin_except_modrinth_without_touching_the_library() {
    let content = json!([
        {"id": "cf12345", "source": "curseforge", "type": "mod", "versionId": "old1", "enabled": true},
        {"id": "otro1", "source": "otra-fuente", "type": "shader", "versionId": "old2", "enabled": true},
        {"id": "local1", "source": "", "type": "resourcepack", "versionId": "old3"},
        // Modrinth, pero con identificadores que nunca se envían a la red.
        {"id": "bad id!", "source": "modrinth", "type": "mod", "versionId": "old4"},
        {"id": 7, "type": "mod", "versionId": "old5"}
    ]);
    let (root, id, state) = library_with(content.clone());
    let result = update_content(&json!({"action": "content_update", "root": root, "id": id})).unwrap();
    assert_eq!(result["updated"], 0);
    assert_eq!(result["instances"][0]["installedContent"], content);
    assert_eq!(result["instances"][0]["name"], "n");
    // Sin cambios no se reescribe la biblioteca ni se crea una copia de seguridad.
    assert_eq!(std::fs::read_to_string(root.join("launcher_state.json")).unwrap(), state);
    assert!(!root.join("launcher_state.json.bak").exists());
    assert!(!root.join("instances").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn content_update_on_an_empty_instance_reports_zero_updates() {
    let (root, id, state) = library_with(json!([]));
    let result = update_content(&json!({"root": root, "id": id})).unwrap();
    assert_eq!(result, json!({"instances": serde_json::from_str::<Value>(&state).unwrap()["instances"], "updated": 0}));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn content_update_rejects_bad_requests_and_libraries_before_any_request() {
    let (root, id, _) = library_with(json!([]));
    let fail = |request: Value| update_content(&request).unwrap_err();
    assert_eq!(fail(json!({"id": id})), "Falta la carpeta de datos.");
    assert_eq!(fail(json!({"root": "relativa", "id": id})), "Carpeta de datos inválida.");
    assert_eq!(fail(json!({"root": root.join("no-existe"), "id": id})), "Carpeta de datos inválida.");
    assert_eq!(fail(json!({"root": root})), "Selecciona una instancia.");
    assert_eq!(fail(json!({"root": root, "id": "../x"})), "Identificador de instancia inválido.");
    assert_eq!(
        fail(json!({"root": root, "id": uuid::Uuid::new_v4().to_string()})),
        "La instancia ya no existe."
    );
    std::fs::remove_dir_all(&root).unwrap();

    // Sin biblioteca.
    let empty = std::env::temp_dir().join(format!("openclient-update-empty-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&empty).unwrap();
    assert_eq!(fail(json!({"root": empty, "id": id})), "La biblioteca está vacía.");
    // Biblioteca ilegible.
    std::fs::write(empty.join("launcher_state.json"), "{roto").unwrap();
    assert!(update_content(&json!({"root": empty, "id": id})).is_err());
    std::fs::remove_dir_all(empty).unwrap();

    // Instancia sin versión de Minecraft o sin lista de contenido.
    for (entry, expected) in [
        (json!({"id": id, "installedContent": []}), "Versión de Minecraft inválida."),
        (json!({"id": id, "mcVersion": "1.21.4"}), "Contenido inválido."),
        (json!({"id": id, "mcVersion": "1.21.4", "installedContent": "no"}), "Contenido inválido."),
    ] {
        let root = std::env::temp_dir().join(format!("openclient-update-bad-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        write_state_locked(&root.join("launcher_state.json"), &json!({"instances": [entry]}).to_string()).unwrap();
        assert_eq!(fail(json!({"root": root, "id": id})), expected);
        std::fs::remove_dir_all(root).unwrap();
    }
}

// opcionales, incompatibles y conflictos

fn annotated(dependencies: Value, installed: Option<Value>, cache: Value) -> Value {
    let mut item = version("project1", "fabric", "1.21.4");
    item["dependencies"] = dependencies;
    let mut summary = project_summary(
        "project1",
        "mod",
        Some("1.21.4"),
        &json!({"id":"project1","project_type":"mod"}),
        &json!([item]),
    )
    .unwrap();
    summary["dependencyNames"] = json!({"dep2": "Dos", "dep3": "Tres", "dep4": "Cuatro"});
    summary["dependencyVersions"] = json!({"dv9": {"project": "dep3", "number": "9"}});
    annotate_versions(summary, installed.as_ref(), &cache)
}

#[test]
fn project_versions_list_optional_and_incompatible_dependencies() {
    let mut dependencies = vec![
        json!({"project_id":"dep1", "dependency_type":"required"}),
        json!({"project_id":"dep2", "dependency_type":"optional"}),
        json!({"version_id":"dv9", "dependency_type":"optional"}),
        json!({"project_id":"bad id", "dependency_type":"optional"}),
        json!({"file_name":"x.jar", "dependency_type":"embedded"}),
        json!({"project_id":"dep4", "version_id":"bad1", "dependency_type":"incompatible"}),
        json!({"project_id":"dep4", "dependency_type":"incompatible"}),
    ];
    let summary = annotated(json!(dependencies), None, Value::Null);
    let version = &summary["versions"][0];
    assert_eq!(
        version["optional"],
        json!([{"project":"dep2","version":null,"title":"Dos"},
               {"project":"dep3","version":"dv9","title":"Tres"}])
    );
    assert_eq!(
        version["incompatible"],
        json!([{"project":"dep4","version":"bad1","title":"Cuatro"},
               {"project":"dep4","version":null,"title":"Cuatro"}])
    );
    // Sin instancia (modpacks) no hay conflictos que calcular.
    assert!(version.get("conflicts").is_none());
    assert_eq!(summary["recommended"]["optional"], version["optional"]);

    dependencies.extend((0..30).map(|n| json!({"project_id": format!("opt{n}"), "dependency_type":"optional"})));
    let capped = annotated(json!(dependencies), None, Value::Null);
    assert_eq!(capped["versions"][0]["optional"].as_array().unwrap().len(), 20);
}

#[test]
fn project_versions_report_declared_and_reverse_conflicts_from_the_offline_cache() {
    let installed = json!([
        {"id":"dep4", "versionId":"v4", "title":"Cuatro", "enabled":true},
        {"id":"old1", "versionId":"o1", "title":"Viejo", "enabled":true},
        {"id":"off1", "versionId":"f1", "title":"Apagado", "enabled":false},
        {"id":"pin1", "versionId":"p1", "title":"Fijado", "enabled":true},
        {"id":"project1", "versionId":"p0", "title":"Yo", "enabled":true}
    ]);
    let cache = json!({
        "version:o1": {"id":"o1", "dependencies":[{"project_id":"project1","dependency_type":"incompatible"}]},
        "version:f1": {"id":"f1", "dependencies":[{"project_id":"project1","dependency_type":"incompatible"}]},
        // Fijada a otra versión: no afecta a `version1`.
        "version:p1": {"id":"p1", "dependencies":[{"project_id":"project1","version_id":"other1","dependency_type":"incompatible"}]},
        "project1": {"id":"p0", "dependencies":[{"project_id":"project1","dependency_type":"incompatible"}]}
    });
    let dependencies = json!([{"project_id":"dep4", "dependency_type":"incompatible"},
        {"project_id":"off1", "dependency_type":"incompatible"}]);
    let summary = annotated(dependencies, Some(installed.clone()), cache);
    assert_eq!(
        summary["versions"][0]["conflicts"],
        json!([{"project":"dep4","title":"Cuatro","reason":"declared"},
               {"project":"old1","title":"Viejo","reason":"reverse"}])
    );
    assert_eq!(summary["recommended"]["conflicts"], summary["versions"][0]["conflicts"]);
    // Una incompatibilidad fijada a esta versión sí cuenta como inversa.
    let pinned = json!({"version:p1": {"id":"p1", "dependencies":[
        {"project_id":"project1","version_id":"version1","dependency_type":"incompatible"}]}});
    let summary = annotated(json!([]), Some(installed), pinned);
    assert_eq!(
        summary["versions"][0]["conflicts"],
        json!([{"project":"pin1","title":"Fijado","reason":"reverse"}])
    );
    // Instancia sin contenido ni caché: lista vacía, no ausente.
    let empty = annotated(json!([]), Some(json!([])), Value::Null);
    assert_eq!(empty["versions"][0]["conflicts"], json!([]));
}

#[test]
fn optional_requests_are_validated_before_any_network_access() {
    let request = |optional: Value| json!({"project": "main1", "optional": optional});
    let ok = request(json!(["a1", "b2"]));
    assert_eq!(validate_optional(&ok, 0).unwrap(), ["a1", "b2"]);
    assert!(validate_optional(&json!({"project": "main1"}), 255).unwrap().is_empty());
    assert!(validate_optional(&request(json!([])), 256).is_err());
    let ids = |count: usize| (0..count).map(|n| format!("m{n}")).collect::<Vec<_>>();
    assert_eq!(validate_optional(&request(json!(ids(16))), 0).unwrap().len(), 16);
    for bad in [
        request(json!(ids(17))),
        request(json!(["a b"])),
        request(json!([""])),
        request(json!([7])),
        request(json!(["dup1", "dup1"])),
        request(json!(["main1"])),
        request(json!("a1")),
    ] {
        assert!(validate_optional(&bad, 0).is_err(), "{bad}");
    }
    // 255 + 1 principal + 1 opcional = 257.
    assert_eq!(
        validate_optional(&request(json!(["a1"])), 255).unwrap_err(),
        "Esta instancia ya tiene 256 proyectos añadidos."
    );
    assert_eq!(validate_optional(&request(json!([])), 255).unwrap().len(), 0);
    assert!(validate_optional(&request(json!(["a1"])), 254).is_ok());
}

fn fabric_library(content: Value) -> (std::path::PathBuf, String) {
    let id = uuid::Uuid::new_v4().to_string();
    let root = std::env::temp_dir().join(format!("openclient-b7-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&root).unwrap();
    let state = json!({"instances": [{
        "id": id, "name": "n", "mcVersion": "1.21.4", "category": "fabric", "installedContent": content
    }]})
    .to_string();
    write_state_locked(&root.join("launcher_state.json"), &state).unwrap();
    (root, id)
}

#[test]
fn content_add_rejects_bad_optional_lists_before_touching_the_network_or_the_library() {
    let full: Vec<Value> = (0..255)
        .map(|n| json!({"id": format!("m{n}"), "versionId": "v1", "type": "mod", "source": "modrinth", "enabled": true}))
        .collect();
    let (root, id) = fabric_library(json!(full));
    let before = std::fs::read_to_string(root.join("launcher_state.json")).unwrap();
    let add = |optional: Value| {
        change(&json!({"action": "content_add", "root": root, "id": id, "project": "main1",
            "kind": "mod", "optional": optional}))
    };
    assert_eq!(add(json!(["opt1"])).unwrap_err(), "Esta instancia ya tiene 256 proyectos añadidos.");
    let too_many: Vec<String> = (0..17).map(|n| format!("o{n}")).collect();
    for bad in [json!(too_many), json!(["a b"]), json!(["opt1", "opt1"]), json!(["main1"]), json!("opt1")] {
        assert!(add(bad.clone()).is_err(), "{bad}");
    }
    assert_eq!(std::fs::read_to_string(root.join("launcher_state.json")).unwrap(), before);
    std::fs::remove_dir_all(root).unwrap();
}

fn conflict_library(cache: Option<Value>) -> (std::path::PathBuf, String) {
    let content = json!([
        {"id":"a1", "versionId":"va", "title":"A", "enabled":true},
        {"id":"b1", "versionId":"vb", "title":"B", "enabled":true},
        {"id":"c1", "versionId":"vc", "title":"C", "enabled":true},
        {"id":"d1", "versionId":"vd", "title":"D", "enabled":false}
    ]);
    let (root, id) = fabric_library(content);
    if let Some(cache) = cache {
        let mods = root.join("instances").join(&id).join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join(".openclient_versions.json"), cache.to_string()).unwrap();
    }
    (root, id)
}

#[test]
fn content_conflicts_compares_active_pairs_with_the_offline_cache() {
    let cache = json!({
        "version:va": {"id":"va", "dependencies":[{"project_id":"b1","dependency_type":"incompatible"}]},
        "version:vc": {"id":"vc", "dependencies":[{"version_id":"vb","dependency_type":"incompatible"},
                                                 {"project_id":"a1","dependency_type":"required"}]},
        "version:vd": {"id":"vd", "dependencies":[{"project_id":"a1","dependency_type":"incompatible"}]}
    });
    let (root, id) = conflict_library(Some(cache));
    let result = content_conflicts(&json!({"root": root, "id": id})).unwrap();
    assert_eq!(result["count"], 2);
    assert_eq!(
        result["conflicts"],
        json!([{"a":"a1","aTitle":"A","b":"b1","bTitle":"B","reason":"incompatible"},
               {"a":"b1","aTitle":"B","b":"c1","bTitle":"C","reason":"incompatible"}])
    );
    assert!(result.get("cacheMissing").is_none());
    std::fs::remove_dir_all(root).unwrap();

    let (root, id) = conflict_library(Some(json!({})));
    assert_eq!(content_conflicts(&json!({"root": root, "id": id})).unwrap(), json!({"conflicts": [], "count": 0}));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn content_conflicts_without_a_usable_cache_reports_it_and_never_fails() {
    let missing = json!({"conflicts": [], "count": 0, "cacheMissing": true});
    let (root, id) = conflict_library(None);
    assert_eq!(content_conflicts(&json!({"root": root, "id": id})).unwrap(), missing);
    // Caché ilegible.
    let mods = root.join("instances").join(&id).join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::write(mods.join(".openclient_versions.json"), "{roto").unwrap();
    assert_eq!(content_conflicts(&json!({"root": root, "id": id})).unwrap(), missing);
    // Las peticiones inválidas siguen fallando.
    assert!(content_conflicts(&json!({"root": root, "id": "../x"})).is_err());
    assert!(content_conflicts(&json!({"id": id})).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn optional_content_origin_is_accepted_by_the_portable_metadata_validation() {
    let entry = |extra: Value| {
        let mut item = json!({"id": "opt1", "versionId": "v1", "type": "mod"});
        for (key, value) in extra.as_object().unwrap() {
            item[key] = value.clone();
        }
        crate::portable_metadata(&json!({"name": "p", "mcVersion": "1.21.4", "category": "fabric",
            "installedContent": [item]}))
    };
    let saved = entry(json!({"addedAs": "optional", "parent": "main1"})).unwrap();
    assert_eq!(saved["installedContent"][0]["addedAs"], "optional");
    assert_eq!(saved["installedContent"][0]["parent"], "main1");
    assert!(entry(json!({"addedAs": "x", "parent": "main1"})).is_err());
}

// install_pack: vínculo y cargadores

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

/// `.mrpack` mínimo (zip sin compresión, una sola entrada): ffi no depende de `zip`.
fn single_entry_zip(name: &str, data: &[u8]) -> Vec<u8> {
    let mut header = Vec::new();
    for field in [20u16, 0, 0, 0, 0x21] {
        header.extend_from_slice(&field.to_le_bytes());
    }
    header.extend_from_slice(&crc32(data).to_le_bytes());
    header.extend_from_slice(&(data.len() as u32).to_le_bytes());
    header.extend_from_slice(&(data.len() as u32).to_le_bytes());
    header.extend_from_slice(&(name.len() as u16).to_le_bytes());
    header.extend_from_slice(&0u16.to_le_bytes());
    let mut out = Vec::new();
    out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(data);
    let start = out.len() as u32;
    let mut central = Vec::new();
    central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    central.extend_from_slice(&20u16.to_le_bytes());
    central.extend_from_slice(&header);
    central.extend_from_slice(&[0; 2 + 2 + 2 + 4]);
    central.extend_from_slice(&0u32.to_le_bytes());
    central.extend_from_slice(name.as_bytes());
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0]);
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[test]
fn installing_a_local_pack_stores_the_modpack_link_for_every_loader() {
    let base = std::env::temp_dir().join(format!("openclient-pack-link-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&base).unwrap();
    for (key, category, loader_version) in [
        (None, "vanilla", None),
        (Some("fabric-loader"), "fabric", Some("0.16.14")),
        (Some("quilt-loader"), "quilt", Some("0.27.1")),
        (Some("forge"), "forge", Some("52.0.1")),
        (Some("neoforge"), "neoforge", Some("21.4.100")),
    ] {
        let mut dependencies = json!({"minecraft": "1.21.4"});
        if let (Some(key), Some(version)) = (key, loader_version) {
            dependencies[key] = json!(version);
        }
        let index = json!({"formatVersion": 1, "game": "minecraft", "versionId": "1.2.3",
            "name": "Mi Pack", "files": [], "dependencies": dependencies});
        let file = base.join(format!("{category}.mrpack"));
        std::fs::write(&file, single_entry_zip("modrinth.index.json", index.to_string().as_bytes())).unwrap();
        let root = base.join(format!("root-{category}"));
        std::fs::create_dir(&root).unwrap();

        for official in [None, Some("pack-oficial")] {
            let mut request = json!({"action": "modpack_import", "root": root, "file": file});
            if let Some(official) = official {
                request["official"] = json!(official);
            }
            let instances = install_pack(&request).unwrap();
            let created = instances.as_array().unwrap().last().unwrap();
            assert_eq!(created["category"], category);
            assert_eq!(
                created["modpack"],
                json!({"source": if official.is_some() { "official" } else { "file" },
                    "projectId": null, "versionId": null, "versionNumber": "1.2.3",
                    "name": "Mi Pack", "loader": category, "loaderVersion": loader_version})
            );
            assert_eq!(created["modpackVersion"], "1.2.3");
            assert_eq!(created["fabricVersion"], if category == "fabric" { json!(loader_version) } else { Value::Null });
            assert_eq!(created.get("official").and_then(Value::as_str), official);
            let sidecar = root.join("instances").join(created["id"].as_str().unwrap()).join(".openclient/modpack.json");
            assert!(sidecar.is_file(), "{category}");
        }
    }
    std::fs::remove_dir_all(base).unwrap();
}
