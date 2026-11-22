# OpenClient

Este proyecto lo hago por aburrimiento. Es un launcher de Minecraft Java para Windows, con la interfaz en C++ (Qt Quick) y el motor en Rust. Todavía está en versión preliminar, así que habrá fallos.

## Qué hace
- Instancias separadas, cada una con su versión de Minecraft, su cargador y, si quieres, su propia cuenta.
- Mods, texturas y shaders desde Modrinth.
- Varias cuentas de Microsoft y perfiles sin conexión.
- Copias de seguridad de tus mundos, con una automática antes de actualizar un modpack.
- Discord Rich Presence.
- Modo portable: con un `portable.txt` junto al programa, todo se guarda a su lado.
- Descarga e instala sus propias actualizaciones desde Ajustes.

## Descargar
El instalador está en la sección [Releases](https://github.com/JoseOrtiz-Dev/OpenClient-launcher/releases/latest) y necesita Windows 10 o superior. No está firmado, así que SmartScreen puede avisar la primera vez. El modpack oficial se descarga al abrirlo por primera vez y se comprueba con SHA-256.

## Compilar
Necesitas Rust (MSVC), Visual Studio Build Tools 2022 con C++, el SDK de Windows, CMake y Qt 6.11.3 o posterior para MSVC 2022 x64, con el módulo Qt Image Formats instalado. Desde la raíz del repositorio, en PowerShell:

```powershell
./launcher/scripts/build-qt.ps1 -QtRoot 'C:/ruta/a/Qt/6.11.3/msvc2022_64' -SmokeTest
```

Eso genera el instalador en `launcher/build/Instalar-OpenClient.exe`.

| Carpeta | Contenido |
|---|---|
| `launcher/apps/qt` | Interfaz: controladores en C++ y páginas en QML |
| `launcher/crates/openclient-core` | Motor: descargas, Java, lanzamiento, cuentas y modpacks |
| `launcher/crates/openclient-ffi` | Puente en C entre el motor y Qt |
| `launcher/scripts` | Compilación, instalador y publicación de versiones |
| `docs` | Cómo publicar una versión |

## Comunidad
Ayuda, novedades y feedback en el [Discord de OpenClient](https://discord.gg/6wGeNxTNS6).

Si quieres colaborar, lee [CONTRIBUTING.md](CONTRIBUTING.md). Para reportar una vulnerabilidad, [SECURITY.md](SECURITY.md). Para sacar una versión nueva, [docs/PUBLICAR-ACTUALIZACION.md](docs/PUBLICAR-ACTUALIZACION.md).

## Licencia
GPL-3.0, en [LICENSE](LICENSE). Qt se enlaza dinámicamente (LGPL-3.0) y los iconos de Octicons llevan su propia licencia en `share/openclient/licenses`.
