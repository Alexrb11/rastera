param(
    [Parameter(Mandatory)][string]$InputPath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    [string]$Executable = '',
    [ValidateRange(1,20)][int]$Runs = 5
)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path $PSScriptRoot -Parent
if (!$Executable) {$Executable = Join-Path $taskRepo 'target/release/rastera.exe'}
$taskExe = (Resolve-Path -LiteralPath $Executable).Path
$taskInput = (Resolve-Path -LiteralPath $InputPath).Path
if (!(Test-Path -LiteralPath $taskInput -PathType Container)) {throw 'La entrada debe ser una carpeta.'}
$taskInfo = & $taskExe --build-info | ConvertFrom-Json
if (!$taskInfo.capture) {throw 'Compila con --features capture para usar el banco de navegación.'}
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $taskOutput) {throw 'El directorio de resultados debe ser nuevo.'}
New-Item -ItemType Directory -Path $taskOutput | Out-Null
$taskSavedData = $env:LOCALAPPDATA
$taskSavedEnv = @{}
Get-ChildItem Env:RASTERA_* | ForEach-Object {$taskSavedEnv[$_.Name] = $_.Value}
$taskSummary = [Collections.Generic.List[object]]::new()
$taskMetadata = @{
    commit = (git -C $taskRepo rev-parse HEAD)
    modified = [bool](git -C $taskRepo status --porcelain)
    executable_sha256 = (Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash.ToLowerInvariant()
    input = $taskInput; executable = $taskExe
    runs = $Runs; viewport_points = '1180x780'; window = 'Hidden'
    cache = 'Rastera; no se purga la caché del sistema operativo'
}
$taskMetadata | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskOutput 'metadata.json') -Encoding utf8
function Invoke-Navigation([string]$taskMode, [int]$taskRun, [string]$taskData) {
    Get-ChildItem Env:RASTERA_* | Remove-Item
    $env:LOCALAPPDATA = $taskData
    $env:RASTERA_NAVIGATION_CSV = Join-Path $taskOutput "$taskMode-$taskRun.csv"
    $env:RASTERA_CAPTURE_SIZE = '1180x780'
    $taskWatch = [Diagnostics.Stopwatch]::StartNew()
    $taskProcess = Start-Process -FilePath $taskExe -ArgumentList ('"'+$taskInput+'"') -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $taskOutput "$taskMode-$taskRun.log")
    $taskWorking = 0L; $taskPrivate = 0L; $taskCpu = 0.0; $taskSamples = 0
    try {
        while (!$taskProcess.HasExited) {
            $taskProcess.Refresh()
            if (!$taskProcess.HasExited) {
                $taskWorking = [Math]::Max($taskWorking,$taskProcess.PeakWorkingSet64)
                $taskPrivate = [Math]::Max($taskPrivate,$taskProcess.PrivateMemorySize64)
                $taskCpu = $taskProcess.TotalProcessorTime.TotalMilliseconds
                $taskSamples++
            }
            if ($taskWatch.Elapsed.TotalSeconds -gt 180) {throw 'La navegación superó 180 segundos.'}
            Start-Sleep -Milliseconds 100
        }
        $taskProcess.WaitForExit()
        if ($taskProcess.ExitCode -ne 0) {throw "Rastera terminó con código $($taskProcess.ExitCode)."}
        $taskRows = @(Import-Csv -LiteralPath $env:RASTERA_NAVIGATION_CSV)
        if ($taskRows.Count -eq 0 -or @($taskRows | Where-Object status -ne 'ready').Count -gt 0 -or !($taskRows | Where-Object scene -eq 'viewer')) {
            throw 'La navegación no completó las escenas sin errores.'
        }
        $taskResult = [PSCustomObject]@{
            cache = $taskMode; run = $taskRun; steps = $taskRows.Count
            duration_ms = $taskWatch.Elapsed.TotalMilliseconds.ToString('F3',[Globalization.CultureInfo]::InvariantCulture)
            sampled_cpu_ms = $taskCpu.ToString('F3',[Globalization.CultureInfo]::InvariantCulture)
            peak_working_set_bytes = $taskWorking; sampled_peak_private_bytes = $taskPrivate
            process_samples = $taskSamples
        }
        Write-Host "$taskMode $taskRun · $($taskRows.Count) pasos · máximo WS $([Math]::Round($taskWorking/1MB,1)) MiB"
        return $taskResult
    } finally {
        $taskProcess.Refresh()
        if (!$taskProcess.HasExited) {
            [void]$taskProcess.CloseMainWindow()
            if (!$taskProcess.WaitForExit(5000)) {Stop-Process -Id $taskProcess.Id}
        }
        $taskProcess.Dispose()
    }
}
try {
    for ($taskRun=1; $taskRun -le $Runs; $taskRun++) {
        $taskSummary.Add((Invoke-Navigation 'cold' $taskRun (Join-Path $taskOutput "data-cold-$taskRun")))
    }
    $taskWarm = Join-Path $taskOutput 'data-warm'
    [void](Invoke-Navigation 'prime' 0 $taskWarm)
    for ($taskRun=1; $taskRun -le $Runs; $taskRun++) {
        $taskSummary.Add((Invoke-Navigation 'warm' $taskRun $taskWarm))
    }
    $taskSummary | Export-Csv -LiteralPath (Join-Path $taskOutput 'process.csv') -NoTypeInformation -Encoding utf8
} finally {
    Get-ChildItem Env:RASTERA_* | Remove-Item
    foreach ($taskName in $taskSavedEnv.Keys) {[Environment]::SetEnvironmentVariable($taskName,$taskSavedEnv[$taskName])}
    if ($null -eq $taskSavedData) {Remove-Item Env:LOCALAPPDATA -ErrorAction SilentlyContinue}
    else {$env:LOCALAPPDATA = $taskSavedData}
}
