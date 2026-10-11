param(
    [ValidateSet('x86_64-pc-windows-gnu', 'x86_64-pc-windows-msvc')]
    [string]$Target = 'x86_64-pc-windows-msvc',
    [string]$Python = 'python',
    [string]$OutputDirectory = 'work/releases'
)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path $PSScriptRoot -Parent
Push-Location $taskRepo
try {
    $taskChanges = & git status --porcelain --untracked-files=normal
    if ($LASTEXITCODE -ne 0 -or $taskChanges) { throw 'Empaqueta desde un checkout limpio para identificar correctamente el código de la build.' }
    $taskOutput = [IO.Path]::GetFullPath($OutputDirectory, $taskRepo)
    New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
    $taskHost = (& rustc -vV | Select-String '^host: ').ToString().Substring(6)
    $taskCompiler = & rustc --version
    if ($LASTEXITCODE -ne 0) { throw 'Rust no está disponible.' }
    if ($taskHost -eq $Target) { cargo build -p rastera-desktop --release --locked }
    else { cargo build -p rastera-desktop --release --locked --target $Target }
    if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación normal de release.' }
    $taskMetadata = Join-Path $taskOutput 'cargo-metadata.json'
    cargo metadata --locked --format-version 1 --filter-platform $Target | Set-Content -LiteralPath $taskMetadata -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw 'No se pudieron enumerar las dependencias.' }
    $taskExe = if ($taskHost -eq $Target) { Join-Path $taskRepo 'target/release/rastera.exe' }
               else { Join-Path $taskRepo "target/$Target/release/rastera.exe" }
    $taskCommit = & git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo identificar el commit.' }
    & $Python (Join-Path $PSScriptRoot 'package-windows.py') --executable $taskExe --metadata $taskMetadata --target $Target --output $taskOutput --commit $taskCommit --compiler $taskCompiler
    if ($LASTEXITCODE -ne 0) { throw 'Falló la creación del paquete portable.' }
} finally {
    Pop-Location
}
