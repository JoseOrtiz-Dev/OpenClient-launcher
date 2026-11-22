import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const privateName = /(?:^|\/)(?:\.env(?:\..*)?|\.dev\.vars(?:\..*)?|credentials[^/]*\.json|client_secret[^/]*\.json|session[^/]*\.(?:json|dpapi)|launcher-state[^/]*\.json)$|\.(?:pem|key|pfx|p12|dpapi|keystore|jks)$/i;
const generatedPath = /(?:^|\/)(?:node_modules|target|dist|entrega|build|cmake-build-[^/]*|\.tools|\.wrangler|\.aws|\.codex|\.openclient|\.albalauncher)(?:\/|$)|^docs\/(?:audits|previews)\//;
const machineFile = /(?:^|\/)CMakeUserPresets\.json$|\.user$|\.(?:exe|msi|pdb|dll|jar)$/i;
const secretRules = [
  ["clave privada", /-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----/],
  ["token de GitHub", /\b(?:gh[pousr]_[A-Za-z0-9]{36,}|github_pat_[A-Za-z0-9_]{60,})\b/],
  ["clave de AWS", /\b(?:AKIA|ASIA)[A-Z0-9]{16}\b/],
  ["credencial literal", /\b(?:AZURE_CLIENT_SECRET|CLOUDFLARE_API_TOKEN|DISCORD_BOT_TOKEN|DISCORD_TOKEN|BOT_TOKEN|TAURI_SIGNING_PRIVATE_KEY|refresh_token|access_token|client_secret|api_key|password)\b["']?\s*[:=]\s*["'][^"'\s${}]{24,}["']/i],
];

export function inspectFile(relative, contents) {
  const normalized = relative.replaceAll("\\", "/");
  const findings = [];
  if ((privateName.test(normalized) && !normalized.endsWith(".example"))
      || generatedPath.test(normalized) || machineFile.test(normalized)) {
    findings.push({ file: normalized, rule: "archivo privado o generado" });
  }
  if (contents.includes(0)) return findings;
  const lines = contents.toString("utf8").split(/\r?\n/);
  for (let i = 0; i < lines.length; i++) {
    for (const [rule, pattern] of secretRules) {
      if (pattern.test(lines[i])) findings.push({ file: normalized, line: i + 1, rule });
    }
  }
  return findings;
}

function localFiles(directory = root) {
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const absolute = path.join(directory, entry.name);
    const relative = path.relative(root, absolute).replaceAll("\\", "/");
    if (entry.name === ".git" || generatedPath.test(relative)
        || relative === "docs/audits" || relative === "docs/previews") continue;
    if (entry.isDirectory()) files.push(...localFiles(absolute));
    else files.push(relative);
  }
  return files;
}

export function checkRepository({ trackedOnly = false } = {}) {
  const repo = spawnSync("git", ["rev-parse", "--show-toplevel"], { cwd: root, encoding: "utf8" });
  const hasRepository = repo.status === 0 && path.resolve(repo.stdout.trim()).toLowerCase() === root.toLowerCase();
  let files;
  if (hasRepository) {
    const args = ["ls-files", "-z", "--cached"];
    if (!trackedOnly) args.push("--others", "--exclude-standard");
    const result = spawnSync("git", args, { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
    if (result.status !== 0) throw new Error("No se pudo consultar el índice de Git.");
    files = [...new Set(result.stdout.split("\0").filter(Boolean))];
  } else {
    if (trackedOnly) throw new Error("La comprobación de archivos versionados requiere un repositorio Git en la raíz.");
    console.log("Sin repositorio Git: revisión local de fuentes; el historial no se ha revisado.");
    files = localFiles();
  }
  const findings = [];
  for (const file of files) {
    const absolute = path.resolve(root, file);
    if (!absolute.startsWith(root + path.sep)) throw new Error("Ruta fuera del repositorio.");
    if (!fs.existsSync(absolute)) continue;
    const info = fs.lstatSync(absolute);
    if (info.isSymbolicLink()) {
      findings.push({ file, rule: "enlace simbólico: revisar antes de publicar" });
      continue;
    }
    if (!info.isFile()) {
      findings.push({ file, rule: "entrada no revisada: directorio o submódulo" });
      continue;
    }
    if (info.size > 5 * 1024 * 1024) {
      findings.push({ file, rule: "archivo mayor de 5 MiB: revisar antes de publicar" });
      continue;
    }
    findings.push(...inspectFile(file, fs.readFileSync(absolute)));
  }
  for (const finding of findings) {
    console.error(`${finding.file}${finding.line ? `:${finding.line}` : ""}: ${finding.rule}`);
  }
  console.log(`${files.length} archivos revisados; ${findings.length} hallazgos. No se imprimen credenciales.`);
  console.log("Comprobación heurística del estado actual; no certifica ausencia de secretos ni analiza el historial.");
  return findings.length === 0;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (!checkRepository({ trackedOnly: process.argv.includes("--tracked") })) process.exitCode = 1;
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
