param(
    [ValidateSet('x86_64-pc-windows-gnu', 'x86_64-pc-windows-msvc')]
    [string]$Target = 'x86_64-pc-windows-msvc',
    [string]$Python = 'python',
    [string]$OutputDirectory = 'work/releases'
)
$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path $PSScriptRoot -Parent
$taskFlags = [Environment]::GetEnvironmentVariable('CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS', 'Process')
Push-Location $taskRepo
try {
    $taskChanges = & git status --porcelain --untracked-files=normal
    if ($LASTEXITCODE -ne 0 -or $taskChanges) { throw 'Empaqueta desde un checkout limpio para identificar correctamente el código de la build.' }
    $taskOutput = [IO.Path]::GetFullPath($OutputDirectory, $taskRepo)
    New-Item -ItemType Directory -Path $taskOutput -Force | Out-Null
    $taskHost = (& rustc -vV | Select-String '^host: ').ToString().Substring(6)
    $taskCompiler = & rustc --version
    if ($LASTEXITCODE -ne 0) { throw 'Rust no está disponible.' }
    $taskExplicitTarget = $taskHost -ne $Target -or $Target.EndsWith('-msvc')
    if ($Target.EndsWith('-msvc')) {
        if ($env:RUSTFLAGS -or $env:CARGO_ENCODED_RUSTFLAGS) { throw 'Retira RUSTFLAGS y CARGO_ENCODED_RUSTFLAGS al generar la distribución MSVC.' }
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
    }
    if (!$taskExplicitTarget) { cargo build -p rastera-desktop --release --locked }
    else { cargo build -p rastera-desktop --release --locked --target $Target }
    if ($LASTEXITCODE -ne 0) { throw 'Falló la compilación normal de release.' }
    $taskMetadata = Join-Path $taskOutput 'cargo-metadata.json'
    cargo metadata --locked --format-version 1 --filter-platform $Target | Set-Content -LiteralPath $taskMetadata -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw 'No se pudieron enumerar las dependencias.' }
    $taskExe = if (!$taskExplicitTarget) { Join-Path $taskRepo 'target/release/rastera.exe' }
               else { Join-Path $taskRepo "target/$Target/release/rastera.exe" }
    $taskCommit = & git rev-parse HEAD
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo identificar el commit.' }
    $taskSysroot = & rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'No se pudo localizar la biblioteca estándar.' }
    $taskRustNotices = Join-Path $taskSysroot 'share/doc/rust/COPYRIGHT-library.html'
    if (!(Test-Path -LiteralPath $taskRustNotices)) { $taskRustNotices = Join-Path $taskSysroot 'share/doc/rust/COPYRIGHT.html' }
    if (!(Test-Path -LiteralPath $taskRustNotices)) { throw 'Instala rust-docs: rustup component add rust-docs.' }
    & $Python (Join-Path $PSScriptRoot 'package-windows.py') --executable $taskExe --metadata $taskMetadata --target $Target --output $taskOutput --commit $taskCommit --compiler $taskCompiler --rust-notices $taskRustNotices
    if ($LASTEXITCODE -ne 0) { throw 'Falló la creación del paquete portable.' }
} finally {
    if ($null -eq $taskFlags) { Remove-Item Env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS -ErrorAction SilentlyContinue }
    else { $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = $taskFlags }
    Pop-Location
}
