param(
    [Parameter(Mandatory = $true)][string]$Archive,
    [string]$ReportsDirectory,
    [ValidateCount(1, 3)][ValidateSet('ubuntu2204', 'ubuntu2404', 'debian12')]
    [string[]]$Service = @('ubuntu2204', 'ubuntu2404', 'debian12'),
    [ValidatePattern('^$|^[a-f0-9]{40}$')][string]$ExpectedCommit = '',
    [switch]$Desktop
)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path -Parent $PSScriptRoot
$taskCompose = Join-Path $PSScriptRoot 'docker/compose.yaml'
$taskArchive = (Resolve-Path -LiteralPath $Archive).Path
$taskName = Split-Path -Leaf $taskArchive
if ($taskName -notmatch '^rastera-\d+\.\d+\.\d+(?:-alpha\.\d+)?-linux-x64\.tar\.gz$') {
    throw 'Supply a Rastera Linux x64 release archive.'
}
if (!(Test-Path -LiteralPath "$taskArchive.sha256" -PathType Leaf)) { throw 'Missing matching .sha256 file.' }
$taskExpected = (Get-Content -LiteralPath "$taskArchive.sha256" -Raw).Trim()
$taskHash = (Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash.ToLowerInvariant()
if ($taskExpected -cne "$taskHash  $taskName") { throw 'The archive does not match its exact SHA-256 sidecar.' }
if (!$ReportsDirectory) { $ReportsDirectory = Join-Path $taskRepo 'work/linux-containers' }
$taskReports = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($ReportsDirectory)
New-Item -ItemType Directory -Path $taskReports -Force | Out-Null
foreach ($taskFolder in @('ubuntu2204', 'ubuntu2404', 'debian12', 'desktop')) {
    New-Item -ItemType Directory -Path (Join-Path $taskReports $taskFolder) -Force | Out-Null
}
function Invoke-RasteraDocker {
    param([string[]]$Arguments)
    & docker @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Docker command failed with exit code $LASTEXITCODE." }
}
$taskDockerOS = & docker info --format '{{.OSType}}'
if ($LASTEXITCODE -ne 0 -or $taskDockerOS -ne 'linux') { throw 'Start Docker Desktop with its Linux engine before running these tests.' }
$taskKeys = @('RASTERA_PACKAGE_DIR', 'RASTERA_ARCHIVE_NAME', 'RASTERA_REPORTS_DIR', 'RASTERA_EXPECTED_COMMIT')
$taskSaved = @{}
foreach ($taskKey in $taskKeys) { $taskSaved[$taskKey] = [Environment]::GetEnvironmentVariable($taskKey, 'Process') }
try {
    $env:RASTERA_PACKAGE_DIR = (Split-Path -Parent $taskArchive).Replace('\', '/')
    $env:RASTERA_ARCHIVE_NAME = $taskName
    $env:RASTERA_REPORTS_DIR = $taskReports.Replace('\', '/')
    $env:RASTERA_EXPECTED_COMMIT = $ExpectedCommit
    $taskBase = @('compose', '-f', $taskCompose)
    Invoke-RasteraDocker -Arguments ($taskBase + @('config', '--quiet'))
    if ($Desktop) {
        Invoke-RasteraDocker -Arguments ($taskBase + @('--profile', 'desktop', 'up', '-d', '--build', '--wait', 'desktop'))
        Write-Output 'Linux desktop: http://127.0.0.1:6080/vnc.html?autoconnect=true&resize=scale'
        Write-Output "Reports: $taskReports"
    } else {
        Invoke-RasteraDocker -Arguments ($taskBase + @('build') + $Service)
        $taskFailures = @()
        foreach ($taskService in $Service) {
            $taskImageID = & docker image inspect "rastera-test-${taskService}:local" --format '{{.Id}}'
            if ($LASTEXITCODE -ne 0) { throw 'The built image could not be inspected.' }
            @{
                service = $taskService; image_id = $taskImageID.Trim(); archive = $taskName
                archive_sha256 = $taskHash; expected_commit = $ExpectedCommit
                compose_sha256 = (Get-FileHash -LiteralPath $taskCompose -Algorithm SHA256).Hash.ToLowerInvariant()
            } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskReports "$taskService/image.json") -Encoding UTF8
            try {
                Invoke-RasteraDocker -Arguments ($taskBase + @('run', '--rm', '--no-deps', $taskService))
            } catch {
                $taskFailures += $taskService
                Write-Warning "The $taskService runtime test failed. Inspect its reports and the Docker output."
            }
        }
        if ($taskFailures.Count) { throw ('Container tests failed: ' + ($taskFailures -join ', ')) }
        Write-Output "All selected Linux container tests passed. Reports: $taskReports"
    }
} finally {
    foreach ($taskKey in $taskKeys) { [Environment]::SetEnvironmentVariable($taskKey, $taskSaved[$taskKey], 'Process') }
}
