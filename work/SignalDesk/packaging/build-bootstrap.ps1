param(
    [Parameter(Mandatory)][string]$Msi,
    [Parameter(Mandatory)][string]$Output
)
$ErrorActionPreference = 'Stop'
$payload = (Resolve-Path -LiteralPath $Msi).Path
$destination = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($Output)
$previous = $env:SIGNALDESK_MSI
try {
    $env:SIGNALDESK_MSI = $payload
    rustc (Join-Path $PSScriptRoot 'bootstrap.rs') --edition=2021 --target x86_64-pc-windows-msvc -C opt-level=z -C strip=symbols -o $destination
    if ($LASTEXITCODE -ne 0) { throw "Setup bootstrap compilation failed with exit code $LASTEXITCODE." }
} finally {
    $env:SIGNALDESK_MSI = $previous
}
