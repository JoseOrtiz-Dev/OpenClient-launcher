//! Diagnóstico de cierres inesperados de Minecraft a partir de su registro.
//!
//! Funciones puras: reciben el texto del log y un pequeño contexto, y devuelven como máximo
//! [`MAX_FINDINGS`] hallazgos ordenados por especificidad (primero los que nombran un mod).
//! Sin red, sin expresiones regulares: solo búsquedas de texto.

use serde::Serialize;

pub const MAX_FINDINGS: usize = 5;
const MAX_NAME: usize = 60;
const MAX_LINE: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    pub id: &'static str,
    pub title: String,
    pub detail: String,
    pub action: String,
    pub severity: Severity,
}

/// Datos de la instancia que ayudan a afinar el consejo. Todo es opcional.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub mc_version: String,
    pub loader: String,
    pub java_major: Option<u32>,
    pub ram_mb: Option<u32>,
    pub system_ram_mb: Option<u64>,
}

impl Context {
    fn minecraft(&self) -> String {
        if self.mc_version.is_empty() {
            "esta versión de Minecraft".into()
        } else {
            format!("Minecraft {}", clean(&self.mc_version).unwrap_or_default())
        }
    }
}

/// Analiza el log y devuelve los hallazgos más específicos primero (máximo 5).
pub fn analyze(log: &str, ctx: &Context) -> Vec<Finding> {
    let lower = log.to_lowercase();
    let deps = dependencies(log);
    let mut scored: Vec<(u8, Finding)> = Vec::new();

    if let Some(finding) = java_version(log, &deps, ctx) {
        scored.push((80, finding));
    }
    scored.extend(dependency_findings(&deps, ctx));
    let reserve = lower.contains("could not reserve enough space") || lower.contains("invalid maximum heap size");
    if reserve {
        scored.push((70, heap_reserve(ctx)));
    } else if log.contains("OutOfMemoryError") {
        scored.push((70, out_of_memory(ctx)));
    }
    scored.extend(mixin(log, ctx));
    scored.extend(duplicate_mods(log, &lower));
    if let Some(finding) = disk_space(&lower) {
        scored.push((65, finding));
    }
    if let Some(finding) = file_permission(log, &lower) {
        scored.push((60, finding));
    }
    if let Some(finding) = graphics(log, &lower) {
        scored.push((60, finding));
    }
    if let Some(finding) = missing_class(log, ctx) {
        scored.push((50, finding));
    }
    if let Some(finding) = network(log, &lower) {
        scored.push((40, finding));
    }
    if !scored.iter().any(|(_, f)| matches!(f.id, "missing_dependency" | "dependency_version" | "minecraft_mismatch")) {
        if let Some(finding) = incompatible_set(log, ctx) {
            scored.push((30, finding));
        }
    }

    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, finding)| finding).take(MAX_FINDINGS).collect()
}

/// Deja solo caracteres razonables y recorta a 60. `None` si no queda nada útil.
fn clean(raw: &str) -> Option<String> {
    let text: String = raw
        .chars()
        .filter(|c| c.is_alphanumeric() || " ._-+'()".contains(*c))
        .take(MAX_NAME * 2)
        .collect();
    let text: String = text.trim().chars().take(MAX_NAME).collect();
    let text = text.trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// Texto hasta la siguiente comilla simple tras `marker` (que debe terminar en comilla).
fn quoted<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let rest = &line[line.find(marker)? + marker.len()..];
    let rest = if marker.ends_with('\'') { rest } else { rest.strip_prefix('\'')? };
    Some(&rest[..rest.find('\'')?])
}

/// Palabra que sigue a `marker`, hasta un separador de log.
fn word_after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let rest = &text[text.find(marker)? + marker.len()..];
    let end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, ']' | ',' | ':' | ')' | ';'))
        .unwrap_or(rest.len());
    Some(rest[..end].trim_matches('\''))
}

fn digits_after(text: &str, marker: &str) -> Option<u32> {
    text.match_indices(marker).find_map(|(at, _)| {
        let rest = &text[at + marker.len()..];
        let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        rest[..end].parse().ok().filter(|v| (52..=100).contains(v))
    })
}

fn first_line<'a>(log: &'a str, needles: &[&str]) -> Option<&'a str> {
    log.lines().find(|line| line.len() <= MAX_LINE && needles.iter().any(|n| line.contains(n)))
}

fn finding(id: &'static str, severity: Severity, title: &str, detail: String, action: String) -> Finding {
    Finding { id, title: title.to_owned(), detail, action, severity }
}

// --- Java ---------------------------------------------------------------------------------

fn java_version(log: &str, deps: &[Dep], ctx: &Context) -> Option<Finding> {
    let from_class = ["class file version ", "class file major version ", "major.minor version "]
        .iter()
        .find_map(|marker| digits_after(log, marker))
        .map(|class| class - 44);
    let from_dep = deps
        .iter()
        .find(|dep| dep.needs.eq_ignore_ascii_case("java"))
        .and_then(|dep| dep.version.as_deref())
        .and_then(|v| v.split(|c: char| !c.is_ascii_digit()).next()?.parse::<u32>().ok())
        .filter(|v| (8..=60).contains(v));
    let needed = from_class.or(from_dep)?;
    let detail = match ctx.java_major {
        Some(current) if current < needed => {
            format!("Esta instancia se ejecutó con Java {current} y algo de lo instalado necesita Java {needed} o más reciente.")
        }
        _ => format!("Minecraft o uno de sus mods necesita Java {needed} o más reciente."),
    };
    Some(finding(
        "java_version",
        Severity::Error,
        "La versión de Java es demasiado antigua",
        detail,
        format!("Abre Ajustes → Java, elige Java {needed} (o superior) e instálala desde ahí si hace falta."),
    ))
}

// --- Memoria ------------------------------------------------------------------------------

fn round_down_512(value: u64) -> u64 {
    value / 512 * 512
}

fn out_of_memory(ctx: &Context) -> Finding {
    let title = "Minecraft se quedó sin memoria";
    let (detail, action) = match (ctx.ram_mb, ctx.system_ram_mb) {
        (Some(ram), Some(system)) => {
            let ram = u64::from(ram);
            let cap = round_down_512(system * 70 / 100);
            let target = (ram * 3 / 2).div_ceil(512) * 512;
            let target = target.min(cap);
            if target > ram {
                (
                    format!("Había {ram} MB asignados y el juego necesitó más."),
                    format!("Sube la RAM de la instancia a unos {target} MB (Ajustes o editar la instancia)."),
                )
            } else {
                (
                    format!("Había {ram} MB asignados de {system} MB que tiene el equipo, así que no queda mucho margen."),
                    "Cierra otros programas, baja la distancia de renderizado o quita mods pesados.".into(),
                )
            }
        }
        (Some(ram), None) => (
            format!("Había {ram} MB asignados y el juego necesitó más."),
            format!("Sube la RAM de la instancia a unos {} MB si el equipo lo permite.", u64::from(ram) + 2048),
        ),
        _ => (
            "El juego necesitó más memoria de la asignada.".into(),
            "Sube la RAM de la instancia en Ajustes o al editarla, sin pasar del 70 % de la del equipo.".into(),
        ),
    };
    finding("out_of_memory", Severity::Error, title, detail, action)
}

fn heap_reserve(ctx: &Context) -> Finding {
    let title = "No se pudo reservar la memoria pedida";
    let (detail, action) = match (ctx.ram_mb, ctx.system_ram_mb) {
        (Some(ram), Some(system)) => {
            let target = round_down_512(system * 50 / 100).max(1024);
            (
                format!("Se pidieron {ram} MB y el equipo tiene {system} MB en total."),
                format!("Baja la RAM de la instancia a unos {target} MB o menos y cierra otros programas."),
            )
        }
        _ => (
            "Java no consiguió la memoria que se le pidió.".into(),
            "Baja la RAM de la instancia: debe ser menor que la memoria libre del equipo.".into(),
        ),
    };
    finding("memory_reserve", Severity::Error, title, detail, action)
}

// --- Dependencias -------------------------------------------------------------------------

struct Dep {
    who: Option<String>,
    needs: String,
    version: Option<String>,
    missing: bool,
}

fn dependencies(log: &str) -> Vec<Dep> {
    let mut deps: Vec<Dep> = Vec::new();
    let mut push = |dep: Dep| {
        if !deps.iter().any(|d| d.needs.eq_ignore_ascii_case(&dep.needs) && d.who == dep.who) {
            deps.push(dep);
        }
    };
    for line in log.lines().filter(|l| l.len() <= MAX_LINE) {
        // Forge/NeoForge: Mod ID: 'a', Requested by: 'b', Expected range: '[1,)', Actual version: '[MISSING]'
        if let (Some(needs), Some(who)) = (quoted(line, "Mod ID: '"), quoted(line, "Requested by: '")) {
            let actual = quoted(line, "Actual version: '").unwrap_or_default();
            if let Some(needs) = clean(needs) {
                push(Dep {
                    who: clean(who),
                    needs,
                    version: quoted(line, "Expected range: '").map(str::to_owned),
                    missing: actual.contains("MISSING"),
                });
            }
            continue;
        }
        let Some(at) = line.find(" requires ") else { continue };
        let (head, rest) = (&line[..at], &line[at + " requires ".len()..]);
        let version = rest
            .strip_prefix("version ")
            .and_then(|v| v.split(" of ").next())
            .or_else(|| rest.starts_with("any version").then_some(""))
            .map(str::to_owned);
        let after_of = rest.find(" of ").map(|i| &rest[i + 4..]);
        let missing = line.contains("is missing") || line.contains("not installed");
        let wrong = line.contains("wrong version");
        if let Some(quoted_who) = quoted(head, "Mod '") {
            let needs = after_of
                .and_then(|r| quoted(r, "mod '").or_else(|| r.split([' ', ',']).next()))
                .and_then(clean);
            if let Some(needs) = needs.filter(|_| missing || wrong) {
                push(Dep { who: clean(quoted_who), needs, version, missing });
            }
        } else if let Some(i) = head.rfind("Mod ") {
            // «Mod X requires Y» sin comillas.
            let who = clean(&head[i + 4..]);
            let needs = rest.split([',', ';', '!', '(']).next().and_then(clean);
            if let (Some(who), Some(needs)) = (who, needs) {
                push(Dep { who: Some(who), needs, version: None, missing: !wrong });
            }
        }
    }
    let api_line = first_line(log, &["Fabric API", "fabric-api"])
        .filter(|l| l.contains("missing") || l.contains("Unmet") || l.contains("not installed"));
    if api_line.is_some() && !deps.iter().any(|d| d.needs.to_lowercase().contains("fabric api") || d.needs.to_lowercase().contains("fabric-api")) {
        deps.push(Dep { who: None, needs: "Fabric API".into(), version: None, missing: true });
    }
    deps
}

fn dependency_findings(deps: &[Dep], ctx: &Context) -> Vec<(u8, Finding)> {
    let mc = ctx.minecraft();
    let mut out = Vec::new();
    let (mut missing, mut wrong) = (0, 0);
    for dep in deps {
        let who = dep.who.as_deref();
        let owner = who.map_or_else(|| "Un mod".to_owned(), |w| format!("«{w}»"));
        let score = if who.is_some() { 100 } else { 88 };
        let needs = dep.needs.as_str();
        let lower = needs.to_lowercase();
        if lower == "java" {
            continue;
        }
        if lower == "minecraft" {
            let wanted = dep
                .version
                .as_deref()
                .and_then(clean)
                .filter(|v| !v.is_empty())
                .map_or_else(|| "otra versión".to_owned(), |v| format!("la versión {v}"));
            let current = if ctx.mc_version.is_empty() { String::new() } else { format!(" y esta instancia usa {}", clean(&ctx.mc_version).unwrap_or_default()) };
            if !out.iter().any(|(_, f): &(u8, Finding)| f.id == "minecraft_mismatch") {
                out.push((
                    92.max(score - 8),
                    finding(
                        "minecraft_mismatch",
                        Severity::Error,
                        "Un mod es de otra versión de Minecraft",
                        format!("{owner} pide Minecraft {wanted}{current}."),
                        format!("Descarga la versión del mod para {mc} o quítalo de la instancia."),
                    ),
                ));
            }
        } else if dep.missing && missing < 2 {
            missing += 1;
            let action = if lower.contains("fabric") && lower.contains("api") {
                format!("Instala Fabric API (Mods → buscar «Fabric API») para {mc}.")
            } else if let Some(w) = who {
                format!("Instala «{needs}» para {mc} o desactiva «{w}».")
            } else {
                format!("Instala «{needs}» para {mc}.")
            };
            out.push((
                score,
                finding(
                    "missing_dependency",
                    Severity::Error,
                    &format!("Falta «{needs}»"),
                    format!("{owner} necesita «{needs}» y no está instalado."),
                    action,
                ),
            ));
        } else if !dep.missing && wrong < 1 {
            wrong += 1;
            let wanted = dep.version.as_deref().and_then(clean).map_or(String::new(), |v| format!(" ({v})"));
            out.push((
                score - 2,
                finding(
                    "dependency_version",
                    Severity::Error,
                    &format!("Versión incompatible de «{needs}»"),
                    format!("{owner} pide otra versión de «{needs}»{wanted}."),
                    format!("Actualiza «{needs}» y los mods que dependen de él a versiones para {mc}."),
                ),
            ));
        }
    }
    out
}

// --- Mixins y mods incompatibles ----------------------------------------------------------

fn mixin(log: &str, ctx: &Context) -> Option<(u8, Finding)> {
    let triggers = [
        "Mixin apply failed",
        "Mixin apply for mod",
        "Mixin transformation of",
        "MixinApplyError",
        "InvalidMixinException",
        "MixinTransformerError",
    ];
    first_line(log, &triggers)?;
    let name = log
        .lines()
        .filter(|l| l.len() <= MAX_LINE && (l.contains("Mixin") || l.contains("mixin")))
        .find_map(|l| word_after(l, "Mixin apply for mod ").or_else(|| word_after(l, "from mod ")))
        .and_then(clean);
    let mc = ctx.minecraft();
    Some(match name {
        Some(name) => (
            90,
            finding(
                "mixin",
                Severity::Error,
                &format!("El mod «{name}» no encaja con el juego"),
                format!("«{name}» no pudo aplicar sus cambios; suele ser una versión pensada para otro Minecraft o chocar con otro mod."),
                format!("Actualiza «{name}» a la versión para {mc} o desactívalo y prueba de nuevo."),
            ),
        ),
        None => (
            45,
            finding(
                "mixin",
                Severity::Error,
                "Un mod no encaja con el juego",
                "Un mod no pudo aplicar sus cambios; suele ser una versión para otro Minecraft o un choque con otro mod.".into(),
                format!("Actualiza los mods para {mc}; si empezó tras añadir uno, desactiva el último."),
            ),
        ),
    })
}

fn incompatible_set(log: &str, ctx: &Context) -> Option<Finding> {
    first_line(log, &["Incompatible mod set", "incompatible with the game or each other", "Incompatible mods found"])?;
    let mc = ctx.minecraft();
    Some(finding(
        "incompatible_mods",
        Severity::Warning,
        "Hay mods que no son compatibles entre sí",
        format!("El cargador detectó mods que no encajan con {mc} o entre ellos."),
        "Desactiva el último mod añadido o actualiza todos a la misma versión de Minecraft.".into(),
    ))
}

fn duplicate_mods(log: &str, lower: &str) -> Option<(u8, Finding)> {
    if !lower.contains("duplicate mod") {
        return None;
    }
    let lines: Vec<&str> = log.lines().collect();
    let at = lines.iter().position(|l| l.to_lowercase().contains("duplicate mod"))?;
    let window = lines[at..lines.len().min(at + 8)].iter().filter(|l| l.len() <= MAX_LINE);
    let mut name = None;
    let mut files = String::new();
    for line in window {
        name = name.or_else(|| {
            quoted(line, "Mod ID: '").or_else(|| quoted(line, "Mod '")).or_else(|| quoted(line, "mod '")).and_then(clean)
        });
        if files.is_empty() {
            if let Some(i) = line.find("mod files: ") {
                files = line[i + 11..].split(',').filter_map(|f| clean(f.trim())).take(3).collect::<Vec<_>>().join(", ");
            }
        }
    }
    let name = name.or_else(|| quoted(lines[at], "'").and_then(clean));
    let (title, detail, score) = match (&name, files.is_empty()) {
        (Some(n), false) => (format!("El mod «{n}» está repetido"), format!("Hay varios archivos del mismo mod: {files}."), 95),
        (Some(n), true) => (format!("El mod «{n}» está repetido"), format!("«{n}» aparece más de una vez en la carpeta de mods."), 95),
        (None, _) => ("Hay mods repetidos".to_owned(), "Algún mod aparece más de una vez en la carpeta de mods.".to_owned(), 60),
    };
    Some((
        score,
        finding("duplicate_mods", Severity::Error, &title, detail, "Deja una sola copia de ese mod (la más reciente) y quita las demás.".into()),
    ))
}

// --- Sistema ------------------------------------------------------------------------------

fn graphics(log: &str, lower: &str) -> Option<Finding> {
    let hit = log.contains("GLFW error")
        || lower.contains("pixel format")
        || lower.contains("does not appear to support opengl")
        || lower.contains("failed to create window")
        || lower.contains("unable to create opengl")
        || lower.contains("glfw_version_unavailable")
        || lower.contains("opengl driver");
    hit.then(|| {
        finding(
            "graphics",
            Severity::Error,
            "Problema con los gráficos",
            "La tarjeta gráfica o su controlador no pudo abrir la ventana del juego (OpenGL).".into(),
            "Actualiza el controlador de la tarjeta gráfica; en portátiles con dos GPU, usa la dedicada para Java.".into(),
        )
    })
}

fn network(log: &str, lower: &str) -> Option<Finding> {
    let hit = lower.contains("failed to download")
        || log.contains("SSLHandshake")
        || log.contains("UnknownHostException");
    hit.then(|| {
        finding(
            "network",
            Severity::Warning,
            "Falló una conexión",
            "No se pudo descargar o contactar con un servidor durante el arranque.".into(),
            "Comprueba la conexión a internet, la fecha del equipo y el antivirus o cortafuegos; luego vuelve a intentarlo.".into(),
        )
    })
}

fn missing_class(log: &str, ctx: &Context) -> Option<Finding> {
    let line = first_line(log, &["NoClassDefFoundError", "NoSuchMethodError", "NoSuchFieldError"])?;
    let kind = ["NoClassDefFoundError", "NoSuchMethodError", "NoSuchFieldError"].iter().find(|k| line.contains(**k))?;
    let item = line[line.find(kind)? + kind.len()..]
        .trim_start_matches(':')
        .trim();
    let token = item.split_whitespace().find(|t| t.contains(['.', '/'])).or_else(|| item.split_whitespace().next());
    let item = token
        .and_then(|t| t.split('(').next())
        .map(|t| t.trim_matches('\''))
        .and_then(|t| t.rsplit(['/', '.']).next())
        .and_then(clean);
    let mc = ctx.minecraft();
    let detail = match item {
        Some(item) => format!("Un mod busca algo que no existe en esta combinación de mods y Minecraft ({item})."),
        None => "Un mod busca algo que no existe en esta combinación de mods y Minecraft.".into(),
    };
    Some(finding(
        "missing_class",
        Severity::Error,
        "Un mod es de otra versión",
        detail,
        format!("Actualiza los mods a las versiones para {mc}; si empezó tras añadir uno, desactiva el último."),
    ))
}

fn file_permission(log: &str, lower: &str) -> Option<Finding> {
    let hit = log.contains("AccessDeniedException")
        || lower.contains("access is denied")
        || lower.contains("being used by another process");
    hit.then(|| {
        finding(
            "file_permission",
            Severity::Error,
            "Sin acceso a un archivo",
            "El sistema no dejó leer o escribir un archivo del juego.".into(),
            "Cierra otras copias de Minecraft y revisa que el antivirus o OneDrive no bloqueen la carpeta de datos.".into(),
        )
    })
}

fn disk_space(lower: &str) -> Option<Finding> {
    let hit = lower.contains("no space left")
        || lower.contains("not enough space")
        || lower.contains("no hay suficiente espacio");
    hit.then(|| {
        finding(
            "disk_space",
            Severity::Error,
            "No hay espacio en el disco",
            "El disco donde se guardan los datos del juego está lleno.".into(),
            "Libera espacio en ese disco o limpia la caché desde Ajustes → Almacenamiento.".into(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        Context {
            mc_version: "1.21.4".into(),
            loader: "fabric".into(),
            java_major: Some(17),
            ram_mb: Some(2048),
            system_ram_mb: Some(16384),
        }
    }

    fn ids(log: &str) -> Vec<&'static str> {
        analyze(log, &ctx()).iter().map(|f| f.id).collect()
    }

    #[test]
    fn old_java_is_deduced_from_class_version() {
        for (class, java) in [(52, 8), (55, 11), (61, 17), (65, 21), (69, 25)] {
            let log = format!(
                "Exception in thread \"main\" java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime (class file version {class}.0), this version of the Java Runtime only recognizes class file versions up to 52.0"
            );
            let found = analyze(&log, &ctx());
            assert_eq!(found[0].id, "java_version");
            assert!(found[0].detail.contains(&format!("Java {java}")), "{}", found[0].detail);
            assert!(found[0].action.contains("Ajustes → Java"));
        }
        let asm = "java.lang.IllegalArgumentException: Unsupported class file major version 65\n\tat org.objectweb.asm.ClassReader";
        assert_eq!(analyze(asm, &ctx())[0].id, "java_version");
    }

    #[test]
    fn heap_problems_use_ram_context() {
        let oom = "[12:00:01] [Render thread/ERROR]: Unable to launch\njava.lang.OutOfMemoryError: Java heap space\n";
        let found = analyze(oom, &ctx());
        assert_eq!(found[0].id, "out_of_memory");
        assert!(found[0].action.contains("3072 MB"), "{}", found[0].action);
        let tight = Context { ram_mb: Some(12288), ..ctx() };
        assert!(analyze(oom, &tight)[0].action.contains("Cierra otros programas"));
        let reserve = "Error occurred during initialization of VM\nCould not reserve enough space for 8388608KB object heap\n";
        let found = analyze(reserve, &Context { ram_mb: Some(8192), system_ram_mb: Some(6000), ..ctx() });
        assert_eq!(found[0].id, "memory_reserve");
        assert!(found[0].action.contains("2560 MB"), "{}", found[0].action);
        assert_eq!(ids(&format!("{reserve}{oom}")), ["memory_reserve"]);
    }

    #[test]
    fn missing_fabric_dependency_names_both_mods() {
        let log = "[main/ERROR]: Incompatible mod set!\nnet.fabricmc.loader.impl.FormattedException: Some of your mods are incompatible with the game or each other!\nA potential solution has been determined:\n\t - Install Fabric API (fabric-api), any version.\nUnmet dependency listing:\n\t - Mod 'Sodium Extra' (sodium-extra) 0.6.0 requires any version of mod 'Sodium' (sodium), which is missing!\n\t - Mod 'Mod Menu' (modmenu) 9.0 requires version 0.100 or later of mod 'Fabric API' (fabric-api), which is missing!\n";
        let found = analyze(log, &ctx());
        assert_eq!(found[0].id, "missing_dependency");
        assert!(found[0].detail.contains("«Sodium Extra»") && found[0].detail.contains("«Sodium»"));
        assert!(found.iter().any(|f| f.title.contains("Fabric API") && f.action.contains("Instala Fabric API")));
        assert!(!found.iter().any(|f| f.id == "incompatible_mods"));
    }

    #[test]
    fn forge_missing_dependency_and_minecraft_mismatch() {
        let log = "Missing or unsupported mandatory dependencies:\n\tMod ID: 'geckolib', Requested by: 'mowziesmobs', Expected range: '[4.4,)', Actual version: '[MISSING]'\n\tMod ID: 'minecraft', Requested by: 'oldmod', Expected range: '[1.20.1]', Actual version: '1.21.4'\n";
        let found = analyze(log, &ctx());
        assert_eq!(found[0].id, "missing_dependency");
        assert!(found[0].detail.contains("mowziesmobs") && found[0].detail.contains("geckolib"));
        let mismatch = found.iter().find(|f| f.id == "minecraft_mismatch").unwrap();
        assert!(mismatch.detail.contains("oldmod") && mismatch.detail.contains("1.21.4"));
    }

    #[test]
    fn fabric_minecraft_and_java_requirements() {
        let log = "\t - Mod 'Create' (create) 6.0 requires version 1.20.1 of Minecraft (minecraft), but only the wrong version is present: 1.21.4!\n\t - Mod 'Iris' (iris) 1.8 requires version 21 or later of Java (java), but only the wrong version is present: 17!\n";
        let found = analyze(log, &ctx());
        assert!(found.iter().any(|f| f.id == "minecraft_mismatch" && f.detail.contains("«Create»") && f.detail.contains("1.20.1")));
        assert!(found.iter().any(|f| f.id == "java_version" && f.detail.contains("Java 21")));
    }

    #[test]
    fn plain_requires_line_is_understood() {
        let log = "Mod Lithium requires Fabric API, version 0.100\n";
        let found = analyze(log, &ctx());
        assert_eq!(found[0].id, "missing_dependency");
        assert!(found[0].detail.contains("«Lithium»"));
    }

    #[test]
    fn mixin_failures_name_the_mod_when_known() {
        let log = "[main/ERROR]: Mixin apply for mod sodium failed sodium.mixins.json:core.MixinFoo from mod sodium -> net.minecraft.client.Y: org.spongepowered.asm.mixin.injection.throwables.InvalidInjectionException\n";
        let found = analyze(log, &ctx());
        assert_eq!(found[0].id, "mixin");
        assert!(found[0].title.contains("«sodium»"));
        let log = "Mixin transformation of net.minecraft.class_310 failed\n";
        let found = analyze(log, &ctx());
        assert_eq!(found[0].id, "mixin");
        assert!(!found[0].title.contains('«'));
        let bracket = "Mixin [lithium.mixins.json:gen.Foo from mod lithium] from phase [DEFAULT] in config [lithium.mixins.json] FAILED during APPLY\norg.spongepowered.asm.mixin.transformer.throwables.MixinTransformerError: An unexpected critical error was encountered";
        assert!(analyze(bracket, &ctx())[0].title.contains("«lithium»"));
    }

    #[test]
    fn generic_incompatible_set_only_without_specific_cause() {
        let log = "[main/ERROR]: Incompatible mod set!\n";
        assert_eq!(ids(log), ["incompatible_mods"]);
    }

    #[test]
    fn system_rules_fire() {
        assert_eq!(ids("GLFW error 65542: WGL: The driver does not appear to support OpenGL."), ["graphics"]);
        assert_eq!(ids("Failed to find a suitable pixel format"), ["graphics"]);
        assert_eq!(ids("java.net.UnknownHostException: resources.download.minecraft.net"), ["network"]);
        assert_eq!(ids("javax.net.ssl.SSLHandshakeException: PKIX path building failed"), ["network"]);
        assert_eq!(ids("Caused by: java.nio.file.AccessDeniedException: C:\\x\\mods\\a.jar"), ["file_permission"]);
        assert_eq!(ids("java.io.IOException: Access is denied"), ["file_permission"]);
        assert_eq!(ids("java.io.IOException: No space left on device"), ["disk_space"]);
        assert_eq!(ids("There is not enough space on the disk"), ["disk_space"]);
        let found = analyze("Caused by: java.lang.NoSuchMethodError: 'void net.minecraft.class_310.method_1(int)'", &ctx());
        assert_eq!(found[0].id, "missing_class");
        let found = analyze("java.lang.NoClassDefFoundError: net/fabricmc/fabric/api/Thing", &ctx());
        assert!(found[0].detail.contains("Thing"));
    }

    #[test]
    fn duplicate_mods_are_named() {
        let log = "Found duplicate mods:\n\tMod ID: 'jei' from mod files: jei-1.21.jar, jei-1.21-old.jar\n";
        let found = analyze(log, &ctx());
        assert_eq!(found[0].id, "duplicate_mods");
        assert!(found[0].title.contains("«jei»") && found[0].detail.contains("jei-1.21-old.jar"));
        let fabric = "net.fabricmc.loader.impl.FormattedException: Duplicate mod 'Sodium' (sodium)";
        assert!(analyze(fabric, &ctx())[0].title.contains("«Sodium»"));
    }

    #[test]
    fn findings_are_capped_ordered_and_names_sanitized() {
        let log = "Mod 'Evil<script>\u{7}Name' (x) requires any version of mod 'Dep X' (dep), which is missing!\n\
                   OutOfMemoryError\nGLFW error 1\nUnknownHostException\nNo space left on device\nAccessDeniedException\nNoSuchMethodError: x\nUnsupportedClassVersionError class file version 65.0\n";
        let found = analyze(log, &ctx());
        assert_eq!(found.len(), MAX_FINDINGS);
        assert_eq!(found[0].id, "missing_dependency");
        assert!(found[0].detail.contains("«EvilscriptName»"), "{}", found[0].detail);
        let long = format!("Mod '{}' (x) requires any version of mod 'B' (b), which is missing!", "a".repeat(200));
        let found = analyze(&long, &ctx());
        assert!(found[0].detail.chars().filter(|c| *c == 'a').count() <= MAX_NAME + 6);
    }

    #[test]
    fn clean_logs_yield_nothing() {
        let log = "[12:00:00] [main/INFO]: Loading Minecraft 1.21.4 with Fabric Loader 0.16\n[12:00:03] [Render thread/INFO]: OpenGL Vendor: NVIDIA\nStopping!\n";
        assert!(analyze(log, &ctx()).is_empty());
        assert!(analyze("", &Context::default()).is_empty());
    }
}
