#!/usr/bin/env bash
set -euo pipefail

smoke=false
appimage=false
while (( $# )); do
    case "$1" in
        --smoke-test) smoke=true ;;
        --appimage) appimage=true ;;
        *) echo "Uso: bash $0 [--smoke-test] [--appimage]" >&2; exit 2 ;;
    esac
    shift
done

launcher=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
arch=$(uname -m)
case "$(uname -s)" in
    Linux) platform=linux; bridge_name=libopenclient_ffi.so ;;
    Darwin)
        platform=macos
        bridge_name=libopenclient_ffi.dylib
        export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-13.0}
        ;;
    *) echo "Usa este script desde Linux o macOS." >&2; exit 2 ;;
esac

if $appimage; then
    if [[ $platform != linux ]]; then
        echo "La opción --appimage solo está disponible en Linux." >&2
        exit 2
    fi
    if ! command -v appimagetool >/dev/null; then
        echo "Instala appimagetool y agrégalo a PATH para generar el AppImage." >&2
        exit 2
    fi
fi

build="$launcher/build/$platform-$arch"
packages="$launcher/build/unix-packages"
mkdir -p "$build" "$packages"
stage=$(mktemp -d "$build/deploy.XXXXXXXX")
trap 'rm -rf -- "$stage"' EXIT

cargo build --release --locked --manifest-path "$launcher/crates/openclient-ffi/Cargo.toml" --target-dir "$build/rust"

args=(
    -DCMAKE_BUILD_TYPE=Release
    "-DOPENCLIENT_BRIDGE=$build/rust/release/$bridge_name"
    "-DOPENCLIENT_UPDATE_URL=${OPENCLIENT_UPDATE_URL:-}"
)
qt_root=${OPENCLIENT_QT_ROOT:-${QT_ROOT_DIR:-}}
if [[ -n $qt_root ]]; then
    args+=("-DCMAKE_PREFIX_PATH=$qt_root")
fi
if [[ -f "$launcher/modpack/official.mrpack" ]]; then
    args+=("-DOPENCLIENT_OFFICIAL_PACK=$launcher/modpack/official.mrpack")
fi
cmake -S "$launcher/apps/qt" -B "$build/qt" "${args[@]}"
cmake --build "$build/qt" --config Release --parallel "${CMAKE_BUILD_PARALLEL_LEVEL:-4}"
cmake --install "$build/qt" --config Release --prefix "$stage"

if [[ $platform == macos ]]; then
    bundle="$stage/OpenClient.app"
    # Firma ad hoc para que los binarios vinculados del bundle se puedan probar.
    codesign --force --deep --sign - "$bundle"
    codesign --verify --deep --strict "$bundle"
    if $smoke; then
        QT_QUICK_BACKEND=software "$bundle/Contents/MacOS/OpenClient" --smoke-test
    fi
    artifact="$packages/OpenClient-macos-$arch.zip"
    ditto -c -k --sequesterRsrc --keepParent "$bundle" "$artifact"
else
    if $smoke; then
        QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software "$stage/bin/openclient-qt" --smoke-test
    fi
    artifact="$packages/OpenClient-linux-$arch.tar.gz"
    tar -czf "$artifact" -C "$stage" .
fi
printf '%s\n' "$artifact"

if $appimage; then
    # Aprovecha las bibliotecas Qt y el puente Rust ya instalados.
    chmod 0755 "$stage"
    ln -s bin/openclient-qt "$stage/AppRun"
    cp "$launcher/src/assets/openclient-cube.svg" "$stage/openclient.svg"
    ln -s openclient.svg "$stage/.DirIcon"
    cat > "$stage/openclient.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=OpenClient
Comment=Launcher de Minecraft Java
Exec=openclient-qt
Icon=openclient
Categories=Game;
Terminal=false
DESKTOP
    appimage_artifact="$packages/OpenClient-linux-$arch.AppImage"
    appimage_args=(--no-appstream)
    if [[ -n ${OPENCLIENT_APPIMAGE_RUNTIME:-} ]]; then
        appimage_args+=(--runtime-file "$OPENCLIENT_APPIMAGE_RUNTIME")
    fi
    ARCH="$arch" APPIMAGE_EXTRACT_AND_RUN=1 appimagetool "${appimage_args[@]}" "$stage" "$appimage_artifact"
    if $smoke; then
        QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software "$appimage_artifact" --appimage-extract-and-run --smoke-test
    fi
    printf '%s\n' "$appimage_artifact"
fi
