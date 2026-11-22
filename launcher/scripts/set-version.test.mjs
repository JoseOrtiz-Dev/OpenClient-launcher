import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./set-version.mjs", import.meta.url));

const fixture = (mainExtra = "") => {
  const root = mkdtempSync(join(tmpdir(), "sv-"));
  mkdirSync(join(root, "apps/qt/src"), { recursive: true });
  mkdirSync(join(root, "scripts"), { recursive: true });
  writeFileSync(
    join(root, "apps/qt/src/main.cpp"),
    `QCoreApplication::setApplicationVersion("0.1.0-preview.27");\nM::compareVersions("0.1.0-preview.27", "0.1.0-preview.26");\n${mainExtra}`,
  );
  writeFileSync(
    join(root, "scripts/build-installer.ps1"),
    `key.SetValue("DisplayVersion", "0.1.0-preview.27");\nfootnote.Text = "0.1.0-preview.27 · Windows x64";\nfootnote.Text = "0.1.0-preview.27 · Windows x64";\n`,
  );
  return root;
};
const run = (root, ...args) => spawnSync(process.execPath, [script, ...args, "--root", root], { encoding: "utf8" });
const read = (root, f) => readFileSync(join(root, f), "utf8");

test("cambia la versión de la aplicación y del instalador, y deja las pruebas intactas", () => {
  const root = fixture();
  const r = run(root, "0.1.0-preview.28");
  assert.equal(r.status, 0, r.stderr);
  const main = read(root, "apps/qt/src/main.cpp");
  assert.match(main, /setApplicationVersion\("0\.1\.0-preview\.28"\)/);
  assert.match(main, /compareVersions\("0\.1\.0-preview\.27", "0\.1\.0-preview\.26"\)/); // fixture de pruebas
  const inst = read(root, "scripts/build-installer.ps1");
  assert.match(inst, /"DisplayVersion", "0\.1\.0-preview\.28"/);
  assert.equal(inst.match(/0\.1\.0-preview\.28 · Windows x64/g).length, 2);
  assert.ok(!inst.includes("preview.27"));
});

test("acepta la etiqueta con v inicial", () => {
  const root = fixture();
  assert.equal(run(root, "v1.2.3").status, 0);
  assert.match(read(root, "apps/qt/src/main.cpp"), /setApplicationVersion\("1\.2\.3"\)/);
});

test("rechaza versiones inválidas sin tocar nada", () => {
  const root = fixture();
  const before = read(root, "apps/qt/src/main.cpp");
  for (const bad of ["", "abc", '1.0"; calc', "1.0 .0", "../x"]) {
    assert.notEqual(run(root, bad).status, 0, bad);
  }
  assert.equal(read(root, "apps/qt/src/main.cpp"), before);
});

test("sin --root lee la versión del primer argumento", () => {
  const r = spawnSync(process.execPath, [script, "v9.9.9-x!"], { encoding: "utf8" });
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /"v9\.9\.9-x!"/);
});

test("falla si el código cambió y ya no encuentra dónde escribir la versión", () => {
  const root = fixture();
  writeFileSync(join(root, "apps/qt/src/main.cpp"), "int main() {}\n");
  const r = run(root, "1.0.0");
  assert.notEqual(r.status, 0);
  assert.match(r.stderr, /main\.cpp/);
});

test("falla si hay más de una línea de versión de la aplicación", () => {
  const root = fixture('QCoreApplication::setApplicationVersion("9.9.9");\n');
  assert.notEqual(run(root, "1.0.0").status, 0);
});
