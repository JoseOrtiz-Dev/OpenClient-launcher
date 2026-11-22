param(
    [string]$QtRoot = $env:OPENCLIENT_QT_ROOT,
    [switch]$SmokeTest,
    [switch]$InstallerOnly,
    [string]$OfficialPack,
    [string]$UpdateUrl
)
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
if (-not $OfficialPack) { $OfficialPack = Join-Path $repository 'modpack/official.mrpack' }
if (-not $QtRoot) {
    $localQt = Join-Path $repository '.tools/Qt/6.11.3/msvc2022_64'
    $parentQt = Join-Path $repository '../../.tools/Qt/6.11.3/msvc2022_64'
    $QtRoot = if (Test-Path -LiteralPath $localQt) { $localQt } else { $parentQt }
}
if (-not (Test-Path -LiteralPath (Join-Path $QtRoot 'lib/cmake/Qt6/Qt6Config.cmake'))) {
    throw 'No se encuentra Qt. Define OPENCLIENT_QT_ROOT con la ruta del kit MSVC 2022 x64 de Qt 6.11.3 o posterior.'
}
$QtRoot = (Resolve-Path -LiteralPath $QtRoot).Path
$cmakeCommand = Get-Command cmake -ErrorAction SilentlyContinue
$cmake = if ($cmakeCommand) { $cmakeCommand.Source } else { $null }
if (-not $cmake) {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Instala CMake o el componente CMake de Visual Studio Build Tools 2022.' }
    $visualStudio = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $visualStudio) { throw 'No se encuentra MSVC x64.' }
    $cmake = Join-Path $visualStudio 'Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin/cmake.exe'
}
if (-not (Test-Path -LiteralPath $cmake)) { throw 'No se encuentra cmake.exe.' }

$nativeBuild = Join-Path $repository 'build/qt'
$rustBuild = Join-Path $repository 'build/rust'
$deployment = Join-Path $repository 'build/qt-preview'
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargo = if ($cargoCommand) { $cargoCommand.Source } else { Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe' }
if (-not (Test-Path -LiteralPath $cargo)) { throw 'No se encuentra cargo.exe.' }
$env:PATH = "$(Split-Path -Parent $cargo);$env:SystemRoot\System32;$env:SystemRoot\System32\WindowsPowerShell\v1.0;$env:PATH"
$ErrorActionPreference = 'Continue'
& $cargo build --release --locked --manifest-path (Join-Path $repository 'crates/openclient-ffi/Cargo.toml') --target-dir $rustBuild
if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación del puente Rust.' }
# El launcher lee el modpack oficial de bin/modpack/official.mrpack (CMake lo instala allí).
if ($UpdateUrl -and $UpdateUrl -notmatch '^https://') { throw 'UpdateUrl debe empezar por https://.' }
$packOption = '-DOPENCLIENT_OFFICIAL_PACK='
if (Test-Path -LiteralPath $OfficialPack -PathType Leaf) {
    $OfficialPack = (Resolve-Path -LiteralPath $OfficialPack).Path
    $packOption += $OfficialPack.Replace('\', '/')
    Write-Output ("Modpack oficial incluido: " + (Get-FileHash -LiteralPath $OfficialPack -Algorithm SHA256).Hash)
} else {
    Write-Warning "No hay modpack oficial en $OfficialPack; el paquete se crea sin él."
}
& $cmake -S (Join-Path $repository 'apps/qt') -B $nativeBuild -G 'Visual Studio 17 2022' -A x64 "-DCMAKE_PREFIX_PATH=$QtRoot" "-DOPENCLIENT_BRIDGE=$rustBuild/release/openclient_ffi.dll" $packOption "-DOPENCLIENT_UPDATE_URL=$UpdateUrl"
if ($LASTEXITCODE -ne 0) { throw 'Falló la configuración de CMake.' }
& $cmake --build $nativeBuild --config Release --parallel 4
if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación de la interfaz Qt.' }
$packageDeployment = Join-Path $repository ("build/qt-package-" + [Guid]::NewGuid().ToString('N'))
try {
    & $cmake --install $nativeBuild --config Release --prefix $packageDeployment
    if ($LASTEXITCODE -ne 0) { throw 'Falló la preparación de las bibliotecas Qt.' }
    if ($SmokeTest) {
        & $cmake -E env QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 "QT_QPA_FONTDIR=$env:SystemRoot/Fonts" (Join-Path $packageDeployment 'bin/openclient-qt.exe') --smoke-test
        if ($LASTEXITCODE -ne 0) { throw 'Falló la comprobación de la vista previa Qt.' }
    }
    & (Join-Path $PSScriptRoot 'build-installer.ps1') -DeploymentDir $packageDeployment -OutputExe (Join-Path $repository 'build/Instalar-OpenClient.exe')

    if (-not $InstallerOnly) {
        $previewExe = Join-Path $deployment 'bin/openclient-qt.exe'
        $runningPreview = Get-Process -Name 'openclient-qt' -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $previewExe }
        if ($runningPreview) {
            Write-Warning 'La vista previa está abierta. El instalador nuevo ya está disponible en build/Instalar-OpenClient.exe.'
        } else {
            & $cmake --install $nativeBuild --config Release --prefix $deployment
            if ($LASTEXITCODE -ne 0) { throw 'El instalador se creó, pero no se pudo actualizar la vista previa.' }
            Write-Output "Vista previa preparada: $previewExe"
        }
    }
} finally {
    $buildPath = Join-Path $repository 'build'
    if ((Split-Path -Parent $packageDeployment) -ne $buildPath -or (Split-Path -Leaf $packageDeployment) -notmatch '^qt-package-[a-f0-9]{32}$') {
        throw 'No se pudo comprobar la carpeta temporal de distribución.'
    }
    if (Test-Path -LiteralPath $packageDeployment) { Remove-Item -LiteralPath $packageDeployment -Recurse -Force }
}
