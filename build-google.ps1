$ErrorActionPreference = 'Stop'
$taskClientId = $env:GEMINI_OAUTH_CLIENT_ID
$taskClientSecret = $env:GEMINI_OAUTH_CLIENT_SECRET
if ([string]::IsNullOrWhiteSpace($taskClientId) -or [string]::IsNullOrWhiteSpace($taskClientSecret)) {
    throw 'Set GEMINI_OAUTH_CLIENT_ID and GEMINI_OAUTH_CLIENT_SECRET before building the Code Assist module'
}
$taskGenerated = Join-Path $PSScriptRoot 'google-module\internal\auth\oauth_client.generated.go'
if (Test-Path -LiteralPath $taskGenerated) { throw 'Remove the existing generated OAuth build input before continuing' }
$taskClientIdLiteral = ConvertTo-Json -InputObject $taskClientId -Compress
$taskClientSecretLiteral = ConvertTo-Json -InputObject $taskClientSecret -Compress
$taskGeneratedSource = "package auth`n`nconst (`n`toauthClientID = $taskClientIdLiteral`n`toauthClientSecret = $taskClientSecretLiteral`n)`n"
[System.IO.File]::WriteAllText($taskGenerated, $taskGeneratedSource, [System.Text.UTF8Encoding]::new($false))
New-Item -ItemType Directory -Path (Join-Path $PSScriptRoot 'engine\plugins') -Force | Out-Null
Push-Location (Join-Path $PSScriptRoot 'google-module')
try {
    $env:CGO_ENABLED = '1'
    $env:GOOS = 'windows'
    $env:GOARCH = 'amd64'
    go test ./... -count=1
    if ($LASTEXITCODE -ne 0) { throw 'Google module tests failed' }
    go vet ./...
    if ($LASTEXITCODE -ne 0) { throw 'Google module vet failed' }
    $taskLibrary = Join-Path $PSScriptRoot 'engine\plugins\gemini-cli.dll'
    go build -trimpath -buildmode=c-shared -ldflags '-s -w -X main.pluginVersion=1.0.5-personal.3' -o $taskLibrary ./cmd/geminicli
    if ($LASTEXITCODE -ne 0) { throw 'Google module build failed' }
} finally {
    Pop-Location
    Remove-Item -LiteralPath $taskGenerated -Force
}
