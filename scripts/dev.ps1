param(
    [ValidateSet('run', 'check', 'test', 'build')]
    [string]$Action = 'run'
)

$ErrorActionPreference = 'Stop'
$taskRepo = Split-Path $PSScriptRoot -Parent
Push-Location $taskRepo
try {
    # A local toolchain may be used without changing the user's global PATH.
    $taskLocalEnv = Join-Path $taskRepo 'work\env.ps1'
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -and (Test-Path -LiteralPath $taskLocalEnv)) {
        . $taskLocalEnv
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw 'Instala Rust y las herramientas de compilación de Windows antes de continuar.'
    }
    switch ($Action) {
        'run' { cargo run -p rastera-desktop --locked }
        'build' { cargo build -p rastera-desktop --release --locked }
        'test' { cargo test --workspace --all-targets --all-features --locked }
        'check' {
            cargo fmt --all -- --check
            if ($LASTEXITCODE -ne 0) { throw 'Formato incorrecto' }
            cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
        }
    }
    if ($LASTEXITCODE -ne 0) { throw "Cargo terminó con código $LASTEXITCODE" }
} finally {
    Pop-Location
}
