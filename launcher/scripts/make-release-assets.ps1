# Prepara lo que se adjunta a un Release de GitHub: manifest.json (para la actualización automática del launcher
# y el aviso del bot) y las notas del Release con el SHA-256 del instalador.
#   ./launcher/scripts/make-release-assets.ps1 -Tag v0.1.0-preview.28 -Repository usuario/OpenClient-launcher `
#       -Installer launcher/build/Instalar-OpenClient.exe -NotesFile RELEASE_NOTES.md -OutDir launcher/build/release
param(
    [Parameter(Mandatory)][string]$Tag,
    [Parameter(Mandatory)][string]$Repository,
    [Parameter(Mandatory)][string]$Installer,
    [Parameter(Mandatory)][string]$NotesFile,
    [Parameter(Mandatory)][string]$OutDir
)
$ErrorActionPreference = 'Stop'

# La etiqueta llega de fuera: se valida antes de usarla en una URL.
if ($Tag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]{1,40})?$') { throw "Etiqueta no válida: $Tag (ejemplo: v0.1.0-preview.28)" }
if ($Repository -notmatch '^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$') { throw "Repositorio no válido: $Repository" }
if (-not (Test-Path -LiteralPath $Installer -PathType Leaf)) { throw "No existe el instalador: $Installer" }
if (-not (Test-Path -LiteralPath $NotesFile -PathType Leaf)) { throw "Falta ${NotesFile}: escribe las notas de la versión antes de crear la etiqueta." }

$notes = (Get-Content -LiteralPath $NotesFile -Raw -Encoding utf8).Trim()
if ($notes.Length -lt 10) { throw "$NotesFile está vacío." }

$version = $Tag.Substring(1)
$url = "https://github.com/$Repository/releases/download/$Tag/$(Split-Path -Leaf $Installer)"
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$manifest = Join-Path $OutDir 'manifest.json'
if (Test-Path -LiteralPath $manifest) { Remove-Item -LiteralPath $manifest }

$script = Join-Path $PSScriptRoot 'make-manifest.mjs'
& node $script --installer $Installer --version $version --url $url --notes $notes --out $manifest
if ($LASTEXITCODE -ne 0) { throw 'Falló make-manifest.mjs.' }

$hash = (Get-FileHash -LiteralPath $Installer -Algorithm SHA256).Hash.ToLower()
$body = "$notes`n`nSHA-256 del instalador: ``$hash```n"
[IO.File]::WriteAllText((Join-Path $OutDir 'release-notes.md'), $body, [Text.UTF8Encoding]::new($false))
Write-Output "Listo en $OutDir (manifest.json, release-notes.md)"
