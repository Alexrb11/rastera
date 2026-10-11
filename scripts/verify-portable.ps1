param(
    [Parameter(Mandatory = $true)][string]$Archive,
    [string]$ReportPath,
    [switch]$InspectOnly
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$taskArchive = (Resolve-Path -LiteralPath $Archive).Path
$taskChecksum = "$taskArchive.sha256"
if (!(Test-Path -LiteralPath $taskChecksum)) { throw 'Falta el archivo SHA-256.' }
$taskExpected = (Get-Content -LiteralPath $taskChecksum -Raw).Split(' ')[0].Trim()
if ($taskExpected -notmatch '^[a-fA-F0-9]{64}$' -or
    (Get-FileHash -LiteralPath $taskArchive -Algorithm SHA256).Hash -ne $taskExpected) {
    throw 'La huella del ZIP no coincide.'
}
$taskRoot = Join-Path ([IO.Path]::GetTempPath()) ('rastera-smoke-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskRoot | Out-Null
$taskZip = [IO.Compression.ZipFile]::OpenRead($taskArchive)
try {
    foreach ($taskEntry in $taskZip.Entries) {
        $taskDestination = [IO.Path]::GetFullPath((Join-Path $taskRoot $taskEntry.FullName))
        if (!$taskDestination.StartsWith($taskRoot + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
            throw 'El ZIP contiene una ruta fuera de la carpeta de extracción.'
        }
    }
} finally { $taskZip.Dispose() }
[IO.Compression.ZipFile]::ExtractToDirectory($taskArchive, $taskRoot)
$taskFolders = @(Get-ChildItem -LiteralPath $taskRoot -Directory)
if ($taskFolders.Count -ne 1) { throw 'Se esperaba una sola carpeta de aplicación.' }
$taskPackage = $taskFolders[0].FullName
foreach ($taskRequired in @('rastera.exe', 'LICENSE', 'LEEME.md', 'BUILD-INFO.json', 'THIRD-PARTY-NOTICES.txt', 'dependency-licenses.json', 'docs/tutorial/index.html')) {
    if (!(Test-Path -LiteralPath (Join-Path $taskPackage $taskRequired) -PathType Leaf)) { throw "Falta $taskRequired" }
}
$taskInfo = Get-Content -LiteralPath (Join-Path $taskPackage 'BUILD-INFO.json') -Raw | ConvertFrom-Json
$taskExe = Join-Path $taskPackage 'rastera.exe'
if ($taskInfo.capture -ne $false -or $taskInfo.version -notmatch '^\d+\.\d+\.\d+-alpha\.\d+$' -or
    $taskInfo.source_commit -notmatch '^[a-f0-9]{40}$' -or
    (Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash -ne $taskInfo.executable_sha256) {
    throw 'Metadatos del ejecutable inválidos.'
}
$taskBytes = [IO.File]::ReadAllBytes($taskExe)
$taskPe = [BitConverter]::ToInt32($taskBytes, 0x3c)
if ([Text.Encoding]::ASCII.GetString($taskBytes, 0, 2) -ne 'MZ' -or
    [BitConverter]::ToUInt32($taskBytes, $taskPe) -ne 0x4550 -or
    [BitConverter]::ToUInt16($taskBytes, $taskPe + 4) -ne 0x8664 -or
    [BitConverter]::ToUInt16($taskBytes, $taskPe + 24 + 68) -ne 2) {
    throw 'Se esperaba un ejecutable gráfico PE x64.'
}
$taskDependencies = @(Get-Content -LiteralPath (Join-Path $taskPackage 'dependency-licenses.json') -Raw | ConvertFrom-Json)
if ($taskDependencies.Count -lt 1) { throw 'El inventario de licencias está vacío.' }
$taskProcess = $null
$taskSaved = @{}
foreach ($taskName in @('PATH', 'LOCALAPPDATA')) { $taskSaved[$taskName] = [Environment]::GetEnvironmentVariable($taskName, 'Process') }
try {
    $env:PATH = "$env:SystemRoot\System32;$env:SystemRoot"
    $env:LOCALAPPDATA = Join-Path $taskRoot 'profile'
    $taskStart = New-Object Diagnostics.ProcessStartInfo
    $taskStart.FileName = $taskExe
    $taskStart.Arguments = '--build-info'
    $taskStart.UseShellExecute = $false
    $taskStart.RedirectStandardOutput = $true
    $taskStart.RedirectStandardError = $true
    $taskProbe = [Diagnostics.Process]::Start($taskStart)
    if (!$taskProbe.WaitForExit(15000)) { $taskProbe.Kill(); throw 'El ejecutable no respondió a --build-info.' }
    $taskBuild = $taskProbe.StandardOutput.ReadToEnd() | ConvertFrom-Json
    if ($taskProbe.ExitCode -ne 0 -or $taskBuild.version -ne $taskInfo.version -or $taskBuild.capture -ne $false) {
        throw 'El ejecutable no coincide con sus metadatos.'
    }
    $taskProbe.Dispose()
    if (!$InspectOnly) {
        $taskSample = (Get-ChildItem -LiteralPath (Join-Path $taskPackage 'docs/tutorial') -Recurse -File | Where-Object Extension -eq '.jpg' | Select-Object -First 1).FullName
        if (!$taskSample) { throw 'Falta una fotografía de práctica.' }
        $taskProcess = Start-Process -FilePath $taskExe -ArgumentList ('"' + $taskSample + '"') -WorkingDirectory $taskPackage -WindowStyle Hidden -PassThru
        $taskCatalog = Join-Path $env:LOCALAPPDATA 'Rastera/catalog.sqlite'
        $taskDeadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            Start-Sleep -Milliseconds 250
            $taskProcess.Refresh()
            if ($taskProcess.HasExited) { throw "La aplicación terminó durante el arranque: $($taskProcess.ExitCode)" }
        } until ((Test-Path -LiteralPath $taskCatalog) -or [DateTime]::UtcNow -gt $taskDeadline)
        if (!(Test-Path -LiteralPath $taskCatalog)) { throw 'No se creó el catálogo del perfil aislado.' }
        Start-Sleep -Seconds 3
        $taskProcess.Refresh()
        if ($taskProcess.HasExited) { throw 'La aplicación terminó después de crear el catálogo.' }
    }
    $taskResult = [ordered]@{
        passed = $true; version = $taskInfo.version; source_commit = $taskInfo.source_commit
        zip_sha256 = $taskExpected; extracted_to = $taskPackage
        startup_checked = !$InspectOnly; isolated_profile = $env:LOCALAPPDATA
        path_without_developer_tools = $true
        scope = 'Integridad y arranque local; no certifica un Windows limpio, renderizado ni accesibilidad.'
    }
    $taskJson = $taskResult | ConvertTo-Json
    if ($ReportPath) { $taskJson | Set-Content -LiteralPath $ReportPath -Encoding UTF8 }
    $taskJson
} finally {
    if ($taskProcess -and !$taskProcess.HasExited) { Stop-Process -Id $taskProcess.Id -Force }
    foreach ($taskName in $taskSaved.Keys) {
        if ($null -eq $taskSaved[$taskName]) { Remove-Item -LiteralPath "Env:$taskName" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($taskName, $taskSaved[$taskName], 'Process') }
    }
}
