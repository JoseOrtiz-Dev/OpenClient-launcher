// Escribe la versión en los únicos sitios donde está fija: la aplicación y el instalador.
// La etiqueta de git (v0.1.0-preview.28) es la fuente de verdad; el workflow de release lo ejecuta antes de compilar.
//
//   node launcher/scripts/set-version.mjs v0.1.0-preview.28 [--root launcher]
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const fail = (message) => {
  console.error(`Error: ${message}`);
  process.exit(1);
};

const argv = process.argv.slice(2);
const rootIndex = argv.indexOf("--root");
const root = rootIndex >= 0 ? argv[rootIndex + 1] : fileURLToPath(new URL("..", import.meta.url));
const raw = argv.find((a, i) => !a.startsWith("--") && i !== (rootIndex >= 0 ? rootIndex + 1 : -1)) ?? "";
const version = raw.replace(/^v/, "");
if (!/^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]{1,40})?$/.test(version)) fail(`versión no válida: "${raw}" (ejemplo: v0.1.0-preview.28)`);

// Cada regla cambia solo la versión y exige el número exacto de coincidencias: si el código cambia de forma,
// falla en lugar de compilar un instalador con la versión equivocada.
const targets = [
  { file: "apps/qt/src/main.cpp", pattern: /(setApplicationVersion\(")[^"]*("\))/g, count: 1 },
  { file: "scripts/build-installer.ps1", pattern: /("DisplayVersion", ")[^"]*(")/g, count: 1 },
  { file: "scripts/build-installer.ps1", pattern: /(footnote\.Text = ")[^"·]*?( · Windows x64")/g, count: 2 },
];

const files = new Map();
for (const { file, pattern, count } of targets) {
  const path = join(root, file);
  let text = files.get(path) ?? readFileSync(path, "utf8");
  const found = text.match(pattern)?.length ?? 0;
  if (found !== count) fail(`${file}: se esperaban ${count} coincidencia(s) de ${pattern} y hay ${found}.`);
  text = text.replace(pattern, (_, before, after) => before + version + after);
  files.set(path, text);
}
for (const [path, text] of files) writeFileSync(path, text);
console.log(`Versión ${version} escrita en ${[...new Set(targets.map((t) => t.file))].join(" y ")}.`);
