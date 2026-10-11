param(
    [string]$Executable = 'target/debug/rastera.exe',
    [string]$CaptureDirectory = 'work/folder-changes',
    [string]$ViewportSize = '1180x780'
)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path $PSScriptRoot -Parent
$taskExe = Join-Path $taskRepo $Executable
$taskInfo = & $taskExe --build-info | ConvertFrom-Json
if (!$taskInfo.capture) { throw 'Compila Rastera con --features capture.' }
$taskOutput = Join-Path $taskRepo $CaptureDirectory
$taskRun = Join-Path $taskOutput ([Guid]::NewGuid().ToString('N'))
$taskImages = Join-Path $taskRun 'imagenes'
New-Item -ItemType Directory -Path $taskImages -Force | Out-Null
$taskExamples = Join-Path $taskRepo 'docs/tutorial/ejemplos'
Copy-Item -LiteralPath (Join-Path $taskExamples '01-taza.png') -Destination (Join-Path $taskImages '01-taza.png')
Copy-Item -LiteralPath (Join-Path $taskExamples '02-cuenco.png') -Destination (Join-Path $taskImages '02-cuenco.png')
$taskSavedData = $env:LOCALAPPDATA
$taskSavedEnv = @{}
Get-ChildItem Env:RASTERA_* | ForEach-Object { $taskSavedEnv[$_.Name] = $_.Value }
try {
    Get-ChildItem Env:RASTERA_* | Remove-Item
    $env:LOCALAPPDATA = Join-Path $taskRun 'datos'
    $env:RASTERA_CAPTURE_TO = Join-Path $taskRun 'actualizada.png'
    $env:RASTERA_CAPTURE_SIZE = $ViewportSize
    $env:RASTERA_CAPTURE_READY_FILE = Join-Path $taskRun 'lista.txt'
    $env:RASTERA_CAPTURE_EXPECT_COUNT = '3'
    $env:RASTERA_CAPTURE_SELECTION = '1'
    $taskProcess = Start-Process -FilePath $taskExe -ArgumentList ('"' + $taskImages + '"') -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $taskRun 'captura.log')
    try {
        $taskDeadline = [DateTime]::UtcNow.AddSeconds(30)
        while (!(Test-Path -LiteralPath $env:RASTERA_CAPTURE_READY_FILE)) {
            if ($taskProcess.HasExited -or [DateTime]::UtcNow -gt $taskDeadline) { throw 'La biblioteca inicial no llegó a estar lista.' }
            Start-Sleep -Milliseconds 50
        }
        # The watcher and initial rendering are ready before any external mutation.
        Copy-Item -LiteralPath (Join-Path $taskExamples '03-jarron.png') -Destination (Join-Path $taskImages '01-taza.png') -Force
        Rename-Item -LiteralPath (Join-Path $taskImages '02-cuenco.png') -NewName '02-cuenco-renombrado.png'
        Copy-Item -LiteralPath (Join-Path $taskExamples '03-jarron.png') -Destination (Join-Path $taskImages '03-nueva.png')
        if (!$taskProcess.WaitForExit(30000)) { throw 'La actualización automática no produjo la captura esperada.' }
        if ($taskProcess.ExitCode -ne 0 -or !(Test-Path -LiteralPath $env:RASTERA_CAPTURE_TO)) { throw 'Falló la captura de cambios externos.' }
        $taskFinal = Join-Path $taskOutput 'biblioteca-actualizada.png'
        [IO.File]::Move($env:RASTERA_CAPTURE_TO, $taskFinal, $true)
        Write-Output "Captura: $taskFinal"
        Write-Output "Datos y copias aisladas: $taskRun"
    } finally {
        if (!$taskProcess.HasExited) { Stop-Process -Id $taskProcess.Id }
        $taskProcess.Dispose()
    }
} finally {
    Get-ChildItem Env:RASTERA_* | Remove-Item
    foreach ($taskName in $taskSavedEnv.Keys) { [Environment]::SetEnvironmentVariable($taskName, $taskSavedEnv[$taskName]) }
    if ($null -eq $taskSavedData) { Remove-Item Env:LOCALAPPDATA -ErrorAction SilentlyContinue }
    else { $env:LOCALAPPDATA = $taskSavedData }
}
