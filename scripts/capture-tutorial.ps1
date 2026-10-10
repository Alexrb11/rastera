param(
    [string]$Executable = 'target/debug/rastera.exe',
    [string]$Python = 'python',
    [string]$ExportDirectory = 'work/tutorial/Para-web',
    [string]$CaptureDirectory = 'docs/tutorial/capturas',
    [string]$ViewportSize = '1180x780'
)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path $PSScriptRoot -Parent
$taskExe = Join-Path $taskRepo $Executable
$taskExamples = Join-Path $taskRepo 'docs/tutorial/ejemplos'
$taskScreenshots = Join-Path $taskRepo $CaptureDirectory
$taskExport = [IO.Path]::GetFullPath((Join-Path $taskRepo $ExportDirectory))
$taskData = Join-Path $taskRepo ('work/tutorial/catalog-' + [Guid]::NewGuid().ToString('N'))
$taskInfo = & $taskExe --build-info | ConvertFrom-Json
if (!$taskInfo.capture) {throw 'Compila Rastera con --features capture.'}
if (Test-Path -LiteralPath $taskExport) {throw 'El destino debe ser nuevo para evitar conflictos con copias anteriores.'}
New-Item -ItemType Directory -Path $taskExport,$taskScreenshots -Force | Out-Null
& $Python (Join-Path $PSScriptRoot 'seed-tutorial.py') $taskData $taskExamples
if ($LASTEXITCODE -ne 0) {throw 'No se pudo preparar el catálogo aislado.'}
$taskNames = @('LOCALAPPDATA','RASTERA_CAPTURE_TO','RASTERA_CAPTURE_FULL','RASTERA_CAPTURE_PANEL','RASTERA_CAPTURE_DEST','RASTERA_CAPTURE_SIZE','RASTERA_CAPTURE_SELECTION','RASTERA_CAPTURE_FILTER','RASTERA_CAPTURE_JOURNAL','RASTERA_CAPTURE_TUTORIAL')
$taskSaved = @{}
foreach ($taskName in $taskNames) {$taskSaved[$taskName] = [Environment]::GetEnvironmentVariable($taskName)}
try {
    foreach ($taskScene in 'biblioteca','visor','organizacion','busqueda','configuracion','revision','resultados') {
        foreach ($taskName in $taskNames) {Remove-Item -LiteralPath ("Env:\" + $taskName) -ErrorAction SilentlyContinue}
        $env:LOCALAPPDATA = $taskData
        $env:RASTERA_CAPTURE_TO = Join-Path $taskData "$taskScene.png"
        $env:RASTERA_CAPTURE_SIZE = $ViewportSize
        if ($taskScene -eq 'biblioteca') {$env:RASTERA_CAPTURE_SELECTION = '1'}
        if ($taskScene -eq 'organizacion') {$env:RASTERA_CAPTURE_PANEL = 'organization'}
        if ($taskScene -eq 'busqueda') {$env:RASTERA_CAPTURE_FILTER = 'aprobada'}
        if ($taskScene -in 'configuracion','revision','resultados') {
            $env:RASTERA_CAPTURE_PANEL = 'batch'
            $env:RASTERA_CAPTURE_DEST = $taskExport
            $env:RASTERA_CAPTURE_TUTORIAL = switch ($taskScene) {'resultados' {'results'} 'revision' {'review'} default {'configure'}}
        }
        $taskInput = if ($taskScene -eq 'visor') {Join-Path $taskExamples '01-taza.png'} else {$taskExamples}
        $taskProcess = Start-Process -FilePath $taskExe -ArgumentList ('"'+$taskInput+'"') -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $taskData "$taskScene.log")
        if (!$taskProcess.WaitForExit(30000)) {Stop-Process -Id $taskProcess.Id; throw "Captura agotada: $taskScene"}
        if ($taskProcess.ExitCode -ne 0 -or !(Test-Path -LiteralPath $env:RASTERA_CAPTURE_TO)) {throw "Captura falló: $taskScene"}
        $taskProcess.Dispose()
        [IO.File]::Move($env:RASTERA_CAPTURE_TO, (Join-Path $taskScreenshots "$taskScene.png"), $true)
        Write-Output "Captura lista: $taskScene"
    }
    Write-Output "Copias reales del lote: $taskExport"
} finally {
    foreach ($taskName in $taskNames) {
        if ($null -eq $taskSaved[$taskName]) {Remove-Item -LiteralPath ("Env:\" + $taskName) -ErrorAction SilentlyContinue}
        else {[Environment]::SetEnvironmentVariable($taskName,$taskSaved[$taskName])}
    }
}
