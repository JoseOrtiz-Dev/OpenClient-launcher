# Contribuir a OpenClient

Gracias por querer ayudar. Esta guía cubre lo mínimo para que un cambio se pueda revisar y fusionar rápido.

## Antes de empezar

- Para algo más grande que un arreglo pequeño, abre primero un issue y cuéntanos qué quieres hacer. Evita que trabajes en algo que ya está en marcha o que no encaja.
- Las vulnerabilidades no van en issues públicos: sigue [SECURITY.md](SECURITY.md).
- Nunca adjuntes tokens, sesiones, `preview.ini` completos ni capturas con tu nombre de usuario de Windows. El botón de exportar informe de Ajustes ya oculta esos datos.

## Preparar el entorno

Necesitas Rust MSVC, Visual Studio Build Tools 2022 con C++, CMake y Qt 6.11.3 o posterior (MSVC 2022 x64). Los pasos están en el [README](README.md). Hoy solo se compila en Windows.

```powershell
./launcher/scripts/build-qt.ps1 -QtRoot 'C:/ruta/a/Qt/6.11.3/msvc2022_64' -SmokeTest
```

## Estructura

| Carpeta | Qué contiene |
| --- | --- |
| `launcher/apps/qt` | Interfaz: C++ (controladores) y QML (páginas y componentes) |
| `launcher/crates/openclient-core` | Motor: descargas, Java, lanzamiento, cuentas, modpacks |
| `launcher/crates/openclient-ffi` | Puente C del motor hacia Qt (ABI v3, JSON) |
| `launcher/scripts` | Compilación, instalador y publicación de versiones |

## Antes de abrir un pull request

```powershell
cargo test --locked --manifest-path launcher/crates/openclient-core/Cargo.toml
cargo test --locked --manifest-path launcher/crates/openclient-ffi/Cargo.toml
cargo clippy --locked --all-targets --manifest-path launcher/crates/openclient-core/Cargo.toml -- -D warnings
cargo clippy --locked --all-targets --manifest-path launcher/crates/openclient-ffi/Cargo.toml -- -D warnings
node launcher/scripts/check-repository.mjs
./launcher/scripts/build-qt.ps1 -QtRoot 'C:/ruta/a/Qt/6.11.3/msvc2022_64' -SmokeTest
```

El smoke test debe terminar en `qml=ok`. Si cambias algo visual, adjunta una captura.

## Criterios de código

- Un cambio, un propósito. Sin refactors ni formato masivo mezclados.
- Reutiliza lo que ya existe antes de añadir otra abstracción.
- Los comentarios explican el motivo, no repiten el código.
- Lo que viene del usuario o de internet (nombres, rutas, URLs, JSON de proyectos) se valida antes de tocar disco o red. Los comandos de instancia nunca se exportan ni se importan.
- Una opción nueva de Ajustes sigue el patrón de las demás: propiedad en `LauncherController`, carga en el constructor, `persist()`, y el control en QML con `Accessible.name`.
- Respeta `Theme.motion`: las animaciones se desactivan con movimiento reducido.
- Textos de interfaz en español, claros y sin jerga.

## Commits

Usa [Conventional Commits](https://www.conventionalcommits.org/es/): `feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `build:`. Una línea de asunto en imperativo, de menos de 72 caracteres.

## Licencia de tus aportes

OpenClient se publica bajo [GPL-3.0](LICENSE). Al enviar un pull request aceptas que tu aporte se distribuya bajo esa misma licencia.
