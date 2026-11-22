# Publicar una actualización del launcher y del modpack

El launcher consulta un `manifest.json` por HTTPS, ofrece «Descargar e instalar» y verifica el SHA-256 antes de abrir el instalador. Nada se ejecuta solo: cada paso lo inicia el usuario con un botón.

## Publicar una versión (automático)

1. Escribe las notas en `RELEASE_NOTES.md` (la raíz del repo): 2 a 4 líneas, lo que verá la gente. Al manifiesto y al aviso del bot solo pasan los primeros 500 caracteres; el Release de GitHub lleva el texto completo.
2. Haz commit y push a `main`.
3. Sube la etiqueta:
   ```powershell
   git tag v0.1.0-preview.28
   git push origin v0.1.0-preview.28
   ```

El workflow `.github/workflows/release.yml` se dispara con la etiqueta y:
- escribe la versión de la etiqueta en la aplicación y el instalador (`launcher/scripts/set-version.mjs`),
- compila con las pruebas de humo y crea `Instalar-OpenClient.exe` (sin el modpack, unos 35 MB),
- genera `manifest.json` con el SHA-256 y el tamaño reales (`launcher/scripts/make-release-assets.ps1`),
- crea el Release de GitHub con el instalador y el manifiesto adjuntos, marcado como el último.

Qué se actualiza solo a partir de ahí:

| Quién | Cómo |
|---|---|
| Botón «Descargar» de la web | Enlaza a `releases/latest/download/Instalar-OpenClient.exe`: siempre sirve el último. No hay que tocar la web. |
| Launcher ya instalado | Consulta `releases/latest/download/manifest.json` (URL fijada al compilar) y ofrece «Descargar e instalar». |
| Bot de Discord | Si su `RELEASE_MANIFEST_URL` apunta al mismo manifiesto, avisa en sus canales en menos de 10 minutos. |

Reglas:
- La etiqueta debe ser `v<mayor>.<menor>.<parche>[-sufijo]`, por ejemplo `v0.1.0-preview.28`. Una distinta no dispara el release.
- El Release no se marca como «pre-release»: GitHub excluye los pre-releases de `releases/latest`, y la web y el launcher dejarían de ver la versión.
- Si falla la compilación no se crea nada: corrige, borra la etiqueta (`git push origin :refs/tags/v…`) y súbela de nuevo.

## Antes de la primera vez
- Sube el modpack oficial (apartado 4), o el launcher no podrá descargarlo.
- El primer Release debe existir antes de que el botón de la web funcione.

## Publicación manual (sin el workflow)

### 1. Compilar con la URL del manifiesto
La URL se fija al compilar y debe ser HTTPS. Si no se indica, la comprobación de actualizaciones queda desactivada.

```powershell
node launcher/scripts/set-version.mjs v0.1.0-preview.28
./launcher/scripts/build-qt.ps1 -QtRoot 'C:/ruta/a/Qt/msvc2022_64' -SmokeTest `
  -UpdateUrl 'https://github.com/<usuario>/<repo>/releases/latest/download/manifest.json'
```

El instalador queda en `launcher/build/Instalar-OpenClient.exe` y no incluye el modpack (se descarga la primera vez).

### 2. Manifiesto y notas
```powershell
./launcher/scripts/make-release-assets.ps1 -Tag v0.1.0-preview.28 -Repository <usuario>/<repo> `
  -Installer launcher/build/Instalar-OpenClient.exe -NotesFile RELEASE_NOTES.md -OutDir launcher/build/release
```
Calcula `sha256` y `size` del instalador real. Si recompilas el instalador, vuelve a generar el manifiesto: el hash corresponde a ese `.exe`.

### 3. Crear el Release
Crea el Release `v<versión>` en GitHub y adjunta `Instalar-OpenClient.exe` y `manifest.json`. El launcher solo acepta descargas del mismo host que el manifiesto o de `github.com`.

## 4. Modpack oficial
Se descarga de un Release aparte (etiqueta `modpack-<versión>`) y su URL, tamaño y SHA-256 están en `launcher/apps/qt/src/LauncherController.cpp` (`kOfficialPackUrl`, `kOfficialPackSize`, `kOfficialPackSha256`). Al publicar un pack nuevo hay que cambiarlos y sacar una versión nueva del launcher.

```powershell
certutil -hashfile launcher/modpack/official.mrpack SHA256
```

## Pendiente
- El instalador no está firmado: Windows SmartScreen puede avisar.
- La instalación silenciosa (`--silent`, `--dir=`) existe en el instalador pero el launcher abre el asistente normal; activarla requiere probar el reemplazo con el launcher cerrado en una máquina limpia.
- El workflow de release no se ha ejecutado nunca en GitHub: la primera vez conviene probarlo con una etiqueta de prueba y borrarla.
