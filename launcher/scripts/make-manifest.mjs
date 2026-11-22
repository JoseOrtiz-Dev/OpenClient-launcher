// Genera o actualiza el manifiesto de actualizaciones que consulta el launcher.
//
//   node launcher/scripts/make-manifest.mjs --installer launcher/build/Instalar-OpenClient.exe \
//     --version 0.1.0-preview.28 \
//     --url https://github.com/<usuario>/<repo>/releases/download/v0.1.0-preview.28/Instalar-OpenClient.exe \
//     [--channel stable|beta] [--notes "Texto corto"] [--out manifest.json]
//
// Si --out ya existe se conservan los demás canales. El launcher exige HTTPS, y para ofrecer
// «Descargar e instalar» necesita sha256 y size; este script los calcula del instalador real.
import { createHash } from "node:crypto";
import { existsSync, readFileSync, statSync, writeFileSync } from "node:fs";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((pairs, value, index, all) => {
    if (value.startsWith("--")) pairs.push([value.slice(2), all[index + 1]]);
    return pairs;
  }, []),
);

const fail = (message) => {
  console.error(`Error: ${message}`);
  process.exit(1);
};

const { installer, version, url } = args;
const channel = args.channel ?? "stable";
const out = args.out ?? "manifest.json";
if (!installer || !version || !url) fail("faltan --installer, --version o --url.");
if (!["stable", "beta"].includes(channel)) fail("--channel debe ser stable o beta.");
if (!/^[0-9]+(\.[0-9]+){0,3}(-[0-9A-Za-z.]{1,40})?$/.test(version)) fail(`versión no válida: ${version}`);
if (!/^https:\/\/[^/\s]+\//.test(url)) fail("--url debe empezar por https://");
if (!existsSync(installer)) fail(`no existe el instalador: ${installer}`);

const size = statSync(installer).size;
if (size <= 0 || size > 1 << 30) fail(`tamaño fuera de rango (máximo 1 GiB): ${size}`);
const sha256 = createHash("sha256").update(readFileSync(installer)).digest("hex");

let manifest = { channels: {} };
if (existsSync(out)) {
  try {
    manifest = JSON.parse(readFileSync(out, "utf8"));
  } catch {
    fail(`${out} existe pero no es JSON válido; no lo sobrescribo.`);
  }
  manifest.channels ??= {};
}

manifest.channels[channel] = { version, url, sha256, size, ...(args.notes ? { notes: args.notes.slice(0, 500) } : {}) };
writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`${out}: canal ${channel} -> ${version}\n  sha256 ${sha256}\n  size   ${size}`);
