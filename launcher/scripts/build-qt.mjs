import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

if (process.platform !== "win32") throw new Error("Esta vista previa utiliza el kit MSVC de Windows.");
const repository = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const windowsPowerShell = path.join(process.env.SystemRoot || "C:/Windows", "System32/WindowsPowerShell/v1.0/powershell.exe");
const shell = fs.existsSync(windowsPowerShell) ? windowsPowerShell : "pwsh.exe";
const result = spawnSync(shell, ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", path.join(repository, "scripts/build-qt.ps1"), ...process.argv.slice(2)], {
  cwd: repository, stdio: "inherit", windowsHide: true
});
if (result.error) console.error("No se pudo ejecutar PowerShell:", result.error.message);
process.exitCode = result.status ?? 1;
