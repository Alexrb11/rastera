param(
    [string]$Executable = 'target/debug/rastera.exe',
    [string]$Python = 'python',
    [string]$CaptureDirectory = 'work/organization/capturas',
    [string]$ViewportSize = '1180x780'
)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path $PSScriptRoot -Parent
$taskExe = Join-Path $taskRepo $Executable
$taskExamples = Join-Path $taskRepo 'docs/tutorial/ejemplos'
$taskScreenshots = Join-Path $taskRepo $CaptureDirectory
New-Item -ItemType Directory -Path $taskScreenshots -Force | Out-Null
$taskInfo = & $taskExe --build-info | ConvertFrom-Json
if (!$taskInfo.capture) {throw 'Compila Rastera con --features capture.'}
$taskSavedData = $env:LOCALAPPDATA
$taskSavedEnv = @{}
Get-ChildItem Env:RASTERA_* | ForEach-Object {$taskSavedEnv[$_.Name] = $_.Value}
try {
    foreach ($taskScene in 'preview','saved') {
        Get-ChildItem Env:RASTERA_* | Remove-Item
        $taskData = Join-Path $taskRepo ('work/organization/catalog-' + [Guid]::NewGuid().ToString('N'))
        & $Python (Join-Path $PSScriptRoot 'seed-tutorial.py') $taskData $taskExamples
        if ($LASTEXITCODE -ne 0) {throw 'No se pudo preparar el catálogo aislado.'}
        $env:LOCALAPPDATA = $taskData
        $env:RASTERA_CAPTURE_TO = Join-Path $taskData "$taskScene.png"
        $env:RASTERA_CAPTURE_SIZE = $ViewportSize
        $env:RASTERA_CAPTURE_PANEL = 'bulk-organization'
        $env:RASTERA_CAPTURE_SELECTION = '1'
        $env:RASTERA_CAPTURE_FILTER = 'aprobada'
        if ($taskScene -eq 'saved') {$env:RASTERA_CAPTURE_BULK = 'apply'}
        $taskProcess = Start-Process -FilePath $taskExe -ArgumentList ('"'+$taskExamples+'"') -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $taskData "$taskScene.log")
        try {
            if (!$taskProcess.WaitForExit(30000)) {throw "Captura agotada: $taskScene"}
            if ($taskProcess.ExitCode -ne 0 -or !(Test-Path -LiteralPath $env:RASTERA_CAPTURE_TO)) {throw "Captura falló: $taskScene"}
            [IO.File]::Move($env:RASTERA_CAPTURE_TO, (Join-Path $taskScreenshots "organization-$taskScene.png"), $true)
            Write-Output "Organización capturada: $taskScene · Catálogo aislado: $taskData"
        } finally {
            if (!$taskProcess.HasExited) {Stop-Process -Id $taskProcess.Id}
            $taskProcess.Dispose()
        }
    }
} finally {
    Get-ChildItem Env:RASTERA_* | Remove-Item
    foreach ($taskName in $taskSavedEnv.Keys) {[Environment]::SetEnvironmentVariable($taskName,$taskSavedEnv[$taskName])}
    if ($null -eq $taskSavedData) {Remove-Item Env:LOCALAPPDATA -ErrorAction SilentlyContinue}
    else {$env:LOCALAPPDATA = $taskSavedData}
}
