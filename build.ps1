$ErrorActionPreference = 'Stop'
$taskRoot = $PSScriptRoot
cargo fmt --manifest-path (Join-Path $taskRoot 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'Rust formatting failed' }
cargo clippy --manifest-path (Join-Path $taskRoot 'Cargo.toml') --all-targets --all-features --locked -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'Rust checks failed' }
cargo test --manifest-path (Join-Path $taskRoot 'Cargo.toml') --locked
if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
cargo build --manifest-path (Join-Path $taskRoot 'Cargo.toml') --release --locked
if ($LASTEXITCODE -ne 0) { throw 'Rust build failed' }
Push-Location (Join-Path $taskRoot 'frontend')
try {
    npm run build
    if ($LASTEXITCODE -ne 0) { throw 'Frontend build failed' }
} finally { Pop-Location }
