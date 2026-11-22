# Avisa de movimiento fuera del sistema: duraciones y curvas escritas a mano
# y ColorAnimation sin MotionColor. Devuelve 1 si encuentra algo.
param([string]$Root = (Join-Path $PSScriptRoot '..\apps\qt\qml'))

# Archivos con movimiento propio que se revisa aparte (bucles, partículas, intro, transición de páginas).
$allowed = @('MotionNumber.qml', 'MotionColor.qml', 'Theme.qml', 'AmbientParticles.qml', 'StartupIntro.qml', 'Reveal.qml', 'FrostedBackdrop.qml')

$findings = foreach ($file in Get-ChildItem -LiteralPath $Root -Recurse -Filter *.qml) {
    if ($allowed -contains $file.Name) { continue }
    $lineNumber = 0
    foreach ($line in Get-Content -LiteralPath $file.FullName -Encoding UTF8) {
        $lineNumber++
        if ($line -match '//\s*motion-ok') { continue }   # excepción justificada en la propia línea
        $code = ($line -replace '//.*$', '')
        if ($code -match '^\s*duration:\s*\d' -or $code -match '\bduration:\s*\d+\s*[;}]') {
            [pscustomobject]@{ File = $file.Name; Line = $lineNumber; Problem = 'duración escrita a mano'; Text = $line.Trim() }
        } elseif ($code -match 'Theme\.motion\s*\?\s*\d+\s*:\s*0') {
            [pscustomobject]@{ File = $file.Name; Line = $lineNumber; Problem = 'compuerta en línea (usa un token)'; Text = $line.Trim() }
        } elseif ($code -match '\bColorAnimation\s*\{' -and $code -notmatch 'MotionColor') {
            [pscustomobject]@{ File = $file.Name; Line = $lineNumber; Problem = 'ColorAnimation sin MotionColor'; Text = $line.Trim() }
        }
    }
}

if ($findings) {
    $findings | Format-Table File, Line, Problem, Text -AutoSize -Wrap | Out-String -Width 220 | Write-Output
    "$(@($findings).Count) avisos de movimiento."
    exit 1
}
'Movimiento: sin avisos.'
