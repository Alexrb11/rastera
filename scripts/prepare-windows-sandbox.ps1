param([Parameter(Mandatory = $true)][string]$Archive)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path $PSScriptRoot -Parent
$taskArchive = (Resolve-Path -LiteralPath $Archive).Path
if (!(Test-Path -LiteralPath "$taskArchive.sha256")) { throw 'Falta la huella del ZIP.' }
$taskRoot = Join-Path $taskRepo ('work/sandbox-' + [guid]::NewGuid().ToString('N'))
$taskInput = Join-Path $taskRoot 'input'
$taskReports = Join-Path $taskRoot 'reports'
New-Item -ItemType Directory -Path $taskInput, $taskReports | Out-Null
Copy-Item -LiteralPath $taskArchive -Destination (Join-Path $taskInput 'Rastera.zip')
Copy-Item -LiteralPath "$taskArchive.sha256" -Destination (Join-Path $taskInput 'Rastera.zip.sha256')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'verify-portable.ps1') -Destination $taskInput
$taskInputXml = [Security.SecurityElement]::Escape($taskInput)
$taskReportsXml = [Security.SecurityElement]::Escape($taskReports)
$taskConfiguration = @"
<Configuration>
  <VGpu>Enable</VGpu>
  <Networking>Disable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <AudioInput>Disable</AudioInput>
  <VideoInput>Disable</VideoInput>
  <PrinterRedirection>Disable</PrinterRedirection>
  <MappedFolders>
    <MappedFolder><HostFolder>$taskInputXml</HostFolder><SandboxFolder>C:\RasteraInput</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$taskReportsXml</HostFolder><SandboxFolder>C:\RasteraReports</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -NoExit -File C:\RasteraInput\verify-portable.ps1 -Archive C:\RasteraInput\Rastera.zip -ReportPath C:\RasteraReports\verification.json</Command></LogonCommand>
</Configuration>
"@
$taskFile = Join-Path $taskRoot 'Rastera.wsb'
$taskConfiguration | Set-Content -LiteralPath $taskFile -Encoding UTF8
Write-Output $taskFile
Write-Output 'Abre este archivo en un equipo con Windows Sandbox habilitado. No se ha ejecutado la prueba.'
Write-Output 'Después del arranque automático, abre rastera.exe desde la ruta extracted_to del informe y completa docs/releasing.md.'
Write-Output 'Solo se comparten el ZIP de entrada (lectura) y una carpeta nueva de informes (escritura).'
