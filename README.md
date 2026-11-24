# OpenClient

Este proyecto lo hago por aburrimiento. Es un launcher de Minecraft Java para Windows, Linux y macOS, con la interfaz en C++ (Qt Quick) y el motor en Rust. Todavía está en versión preliminar, así que habrá fallos.

## Qué hace
- Instancias separadas, cada una con su versión de Minecraft, su cargador y, si quieres, su propia cuenta.
- Mods, texturas y shaders desde Modrinth.
- Varias cuentas de Microsoft y perfiles sin conexión.
- Copias de seguridad de tus mundos, con una automática antes de actualizar un modpack.
- Discord Rich Presence.
- Con un `portable.txt` junto al ejecutable, al `AppImage` o a `OpenClient.app`, el modo portable guarda los datos en ese mismo lugar.
- Consulta las actualizaciones desde Ajustes. Windows puede instalarlas desde el launcher y Linux y macOS abren la página de la nueva versión.

## Descargar
Los paquetes se publican en [Releases](https://github.com/JoseOrtiz-Dev/OpenClient-launcher/releases/latest).

| Sistema | Paquete |
|---|---|
| Windows 10 o superior, x64 | `Instalar-OpenClient.exe`. SmartScreen puede avisar porque el instalador carece de firma. |
| Linux x64 | Descarga `OpenClient-linux-x86_64.AppImage`, dale permiso de ejecución y ábrelo. También puedes extraer `OpenClient-linux-x86_64.tar.gz` y ejecutar `bin/openclient-qt`. Los paquetes se compilan en Ubuntu 24.04. |
| macOS 13 o superior | Extrae `OpenClient-macos-arm64.zip` para Apple Silicon o `OpenClient-macos-x86_64.zip` para Intel. Los paquetes llevan firma ad hoc y quedan pendientes de notarización. |

Para abrir el AppImage desde la terminal, ejecuta `chmod +x OpenClient-linux-x86_64.AppImage` una vez y luego `./OpenClient-linux-x86_64.AppImage`. Si FUSE no está disponible, añade `--appimage-extract-and-run` al comando de arranque.

Las cuentas Microsoft usan el almacén de credenciales del usuario en cada sistema. En Linux se necesita Secret Service y la utilidad `secret-tool` (paquete `libsecret-tools` en Debian y Ubuntu). El modpack oficial se descarga al abrirlo por primera vez y se comprueba con SHA-256. Las versiones antiguas de Minecraft con bibliotecas nativas solo para Intel pueden requerir Rosetta y Java x64 en Apple Silicon.

## Compilar en Windows
Necesitas Rust (MSVC), Visual Studio Build Tools 2022 con C++, el SDK de Windows, CMake y Qt 6.11.3 o posterior para MSVC 2022 x64, con el módulo Qt Image Formats instalado. Desde la raíz del repositorio, en PowerShell:

```powershell
./launcher/scripts/build-qt.ps1 -QtRoot 'C:/ruta/a/Qt/6.11.3/msvc2022_64' -SmokeTest
```

Eso genera el instalador en `launcher/build/Instalar-OpenClient.exe`.

## Compilar en Linux o macOS

Necesitas Rust, CMake, un compilador C++ y Qt 6.11.3 o posterior con Qt Image Formats. En Linux instala también `patchelf` y los encabezados de OpenGL y OpenSSL. En macOS se usa Xcode. Desde la raíz del repositorio:

```bash
OPENCLIENT_QT_ROOT=/ruta/al/kit/Qt bash launcher/scripts/build-qt-unix.sh --smoke-test
```

Si tienes [AppImageTool](https://github.com/AppImage/appimagetool/releases/tag/1.9.1) disponible en el `PATH` como `appimagetool`, añade `--appimage` al comando anterior en Linux. Los paquetes aparecen en `launcher/build/unix-packages`. GitHub Actions compila y prueba el AppImage y el archivo `.tar.gz` de Linux x64, además de los paquetes de Mac Intel y Apple Silicon.

| Carpeta | Contenido |
|---|---|
| `launcher/apps/qt` | Interfaz: controladores en C++ y páginas en QML |
| `launcher/crates/openclient-core` | Motor: descargas, Java, lanzamiento, cuentas y modpacks |
| `launcher/crates/openclient-ffi` | Puente en C entre el motor y Qt |
| `launcher/scripts` | Compilación, instalador y publicación de versiones |

## Comunidad
Ayuda, novedades y feedback en el [Discord de OpenClient](https://discord.gg/6wGeNxTNS6).

Si quieres colaborar, lee [CONTRIBUTING.md](CONTRIBUTING.md). Para reportar una vulnerabilidad, [SECURITY.md](SECURITY.md).
## Licencia
GPL-3.0, en [LICENSE](LICENSE). Qt se enlaza dinámicamente (LGPL-3.0) y los iconos de Octicons llevan su propia licencia en `share/openclient/licenses`.
