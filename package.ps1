param(
    [string]$Packager = 'cpr-plugin.exe',
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'dist-package')
)
$ErrorActionPreference = 'Stop'
$taskTarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $PSScriptRoot 'target' }
$taskBinary = Join-Path $taskTarget 'release\gemini-bridge.exe'
if (-not (Test-Path -LiteralPath $taskBinary)) { throw 'Run build.ps1 first' }
Push-Location $PSScriptRoot
try {
    & $Packager package --manifest plugin.json --binary $taskBinary --target x86_64-pc-windows-msvc --output-dir $OutputDirectory --resource-map 'web=frontend/dist' --resource-map 'engine=engine' --resource-map 'legal=legal'
    if ($LASTEXITCODE -ne 0) { throw 'Plugin packaging failed' }
} finally { Pop-Location }
