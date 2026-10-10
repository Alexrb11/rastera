param(
    [Parameter(Mandatory)][string]$InputPath,
    [string]$Executable = '',
    [Parameter(Mandatory)][string]$OutputPath,
    [ValidateRange(1,20)][int]$Runs = 5,
    [ValidateRange(5,300)][int]$Seconds = 10,
    [ValidateSet('cold','warm','both')][string]$Cache = 'both',
    [switch]$Full
)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path $PSScriptRoot -Parent
if (!$Executable) { $Executable = Join-Path $taskRepo 'target/release/rastera.exe' }
$taskExecutable = (Resolve-Path -LiteralPath $Executable).Path
if ($Full) {
    $taskBuild = (& $taskExecutable --build-info | ConvertFrom-Json)
    if (!$taskBuild.capture) { throw 'Para -Full, compila con cargo build -p rastera-desktop --release --features capture --locked.' }
}
$taskInput = (Resolve-Path -LiteralPath $InputPath).Path
$taskOutput = [IO.Path]::GetFullPath($OutputPath)
if (Test-Path -LiteralPath $taskOutput) { throw 'El CSV de salida ya existe.' }
$taskSession = Join-Path ([IO.Path]::GetDirectoryName($taskOutput)) ('rastera-native-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskSession | Out-Null
$taskCores = [Environment]::ProcessorCount
$taskCommit = git -C $taskRepo rev-parse HEAD
$taskModified = [bool](git -C $taskRepo status --porcelain)
$taskOriginalData = $env:LOCALAPPDATA
$taskOriginalFull = $env:RASTERA_CAPTURE_FULL
$taskOriginalCapture = $env:RASTERA_CAPTURE_TO
$taskOriginalJournal = $env:RASTERA_CAPTURE_JOURNAL
$taskOriginalPanel = $env:RASTERA_CAPTURE_PANEL
$taskResults = [Collections.Generic.List[object]]::new()
function Measure-RasteraWindow([string]$taskData, [string]$taskMode, [int]$taskRun) {
    $env:LOCALAPPDATA = $taskData
    $env:RASTERA_CAPTURE_TO = $null
    $env:RASTERA_CAPTURE_JOURNAL = $null
    $env:RASTERA_CAPTURE_PANEL = $null
    $env:RASTERA_CAPTURE_FULL = if ($Full) { '1' } else { $null }
    $taskWatch = [Diagnostics.Stopwatch]::StartNew()
    $taskProcess = Start-Process -FilePath $taskExecutable -ArgumentList ('"' + $taskInput + '"') -WindowStyle Hidden -PassThru -RedirectStandardError (Join-Path $taskSession "$taskMode-$taskRun.log")
    $taskPeakPrivate = 0L
    $taskPeakWorking = 0L
    $taskIdleCpu = $null
    $taskIdleWall = 0.0
    try {
        while ($taskWatch.Elapsed.TotalSeconds -lt $Seconds) {
            $taskProcess.Refresh()
            if ($taskProcess.HasExited) { throw "Rastera terminó antes de completar la medición: $($taskProcess.ExitCode)" }
            $taskPeakPrivate = [Math]::Max($taskPeakPrivate, $taskProcess.PrivateMemorySize64)
            $taskPeakWorking = [Math]::Max($taskPeakWorking, $taskProcess.PeakWorkingSet64)
            if ($null -eq $taskIdleCpu -and $taskWatch.Elapsed.TotalSeconds -ge ($Seconds - 2)) {
                $taskIdleCpu = $taskProcess.TotalProcessorTime.TotalMilliseconds
                $taskIdleWall = $taskWatch.Elapsed.TotalMilliseconds
            }
            Start-Sleep -Milliseconds 100
        }
        $taskProcess.Refresh()
        $taskCpu = $taskProcess.TotalProcessorTime.TotalMilliseconds
        $taskIdleDuration = $taskWatch.Elapsed.TotalMilliseconds - $taskIdleWall
        $taskResult = [PSCustomObject]@{
            cache = $taskMode; run = $taskRun; full = [bool]$Full
            duration_ms = [Math]::Round($taskWatch.Elapsed.TotalMilliseconds,3)
            cpu_total_ms = [Math]::Round($taskCpu,3)
            idle_cpu_one_core_percent = [Math]::Round(100 * ($taskCpu - $taskIdleCpu) / $taskIdleDuration,3)
            idle_cpu_machine_percent = [Math]::Round(100 * ($taskCpu - $taskIdleCpu) / $taskIdleDuration / $taskCores,3)
            peak_working_set_bytes = [Math]::Max($taskPeakWorking,$taskProcess.PeakWorkingSet64)
            sampled_peak_private_bytes = [Math]::Max($taskPeakPrivate,$taskProcess.PrivateMemorySize64)
            final_working_set_bytes = $taskProcess.WorkingSet64
            final_private_bytes = $taskProcess.PrivateMemorySize64
            logical_cpus = $taskCores; commit = $taskCommit; modified = $taskModified
            input_path = $taskInput; executable = $taskExecutable
        }
        Write-Host "$taskMode $taskRun/$Runs · WS máximo $([Math]::Round($taskResult.peak_working_set_bytes/1MB,1)) MiB · CPU final $($taskResult.idle_cpu_one_core_percent)% de un núcleo"
        return $taskResult
    } finally {
        if (!$taskProcess.HasExited) {
            [void]$taskProcess.CloseMainWindow()
            if (!$taskProcess.WaitForExit(5000)) { Stop-Process -Id $taskProcess.Id }
        }
        $taskProcess.Dispose()
    }
}
try {
    if ($Cache -in @('cold','both')) {
        for ($taskRun = 1; $taskRun -le $Runs; $taskRun++) {
            $taskResults.Add((Measure-RasteraWindow (Join-Path $taskSession "cold-$taskRun") 'cold' $taskRun))
        }
    }
    if ($Cache -in @('warm','both')) {
        $taskWarmData = Join-Path $taskSession 'warm'
        # Prime the application cache; this observation is explicitly excluded from results.
        [void](Measure-RasteraWindow $taskWarmData 'prime' 0)
        for ($taskRun = 1; $taskRun -le $Runs; $taskRun++) {
            $taskResults.Add((Measure-RasteraWindow $taskWarmData 'warm' $taskRun))
        }
    }
    # Exclusive creation prevents a competing writer from being replaced.
    $taskFile = [IO.File]::Open($taskOutput,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write)
    $taskWriter = [IO.StreamWriter]::new($taskFile)
    try { $taskResults | ConvertTo-Csv -NoTypeInformation | ForEach-Object { $taskWriter.WriteLine($_) } }
    finally { $taskWriter.Dispose() }
} finally {
    $env:LOCALAPPDATA = $taskOriginalData
    $env:RASTERA_CAPTURE_FULL = $taskOriginalFull
    $env:RASTERA_CAPTURE_TO = $taskOriginalCapture
    $env:RASTERA_CAPTURE_JOURNAL = $taskOriginalJournal
    $env:RASTERA_CAPTURE_PANEL = $taskOriginalPanel
}
