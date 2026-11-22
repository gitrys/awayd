# Sign the ZIP of a DRAFT GitHub release with minisign (rsign2) and upload the signature.
#
#   .\packaging\windows\sign-release.ps1 -Tag v0.1.0 [-SecretKey <path>]
#
# Requires: gh (logged in), rsign (`cargo install rsign2 --locked`), minisign.pub in the
# repository root. The secret key stays on this machine; rsign asks for its password.
# The script never publishes: review the draft afterwards and publish it yourself.
param(
    [Parameter(Mandatory = $true)][string]$Tag,
    [string]$SecretKey = (Join-Path $env:USERPROFILE ".minisign\awayd.key"),
    [string]$Repo = "gitrys/awayd"
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$PubKey = Join-Path $Root "minisign.pub"

function Assert-NativeSuccess([string]$What) {
    if ($LASTEXITCODE -ne 0) { throw "$What failed (exit code $LASTEXITCODE)" }
}

foreach ($cmd in @("gh", "rsign")) {
    if (-not (Get-Command $cmd -ErrorAction SilentlyContinue)) { throw "'$cmd' not found on PATH" }
}
if (-not (Test-Path $PubKey)) { throw "public key not found: $PubKey" }
if (-not (Test-Path $SecretKey)) { throw "secret key not found: $SecretKey (use -SecretKey)" }

$Work = Join-Path ([System.IO.Path]::GetTempPath()) "awayd-sign-$Tag"
if (Test-Path $Work) { Remove-Item -Recurse -Force $Work }
New-Item -ItemType Directory -Path $Work | Out-Null

gh release download $Tag -R $Repo -D $Work -p "*.zip" -p "SHA256SUMS.txt"
Assert-NativeSuccess "gh release download"

# Verify the downloaded ZIP against SHA256SUMS.txt before signing it.
$Zips = @(Get-ChildItem (Join-Path $Work "*.zip"))
if ($Zips.Count -ne 1) { throw "expected exactly one ZIP in release $Tag, found $($Zips.Count)" }
$Zip = $Zips[0]
$Line = Get-Content (Join-Path $Work "SHA256SUMS.txt") | Where-Object { $_ -match "\s\*?$([regex]::Escape($Zip.Name))$" } | Select-Object -First 1
if (-not $Line) { throw "SHA256SUMS.txt has no entry for $($Zip.Name)" }
$Expected = ($Line -split '\s+')[0].ToLowerInvariant()
$Actual = (Get-FileHash -Algorithm SHA256 $Zip.FullName).Hash.ToLowerInvariant()
if ($Expected -ne $Actual) { throw "checksum mismatch for $($Zip.Name): expected $Expected, got $Actual" }
Write-Host "Checksum OK: $($Zip.Name)"

$Sig = "$($Zip.FullName).minisig"
rsign sign -s $SecretKey -p $PubKey -x $Sig -t "awayd $Tag $($Zip.Name)" $Zip.FullName
Assert-NativeSuccess "rsign sign"
rsign verify -p $PubKey -x $Sig $Zip.FullName
Assert-NativeSuccess "rsign verify"

gh release upload $Tag $Sig -R $Repo --clobber
Assert-NativeSuccess "gh release upload"

Write-Host ""
Write-Host "Signature uploaded: $(Split-Path -Leaf $Sig)"
Write-Host "Review the draft on GitHub, then publish it with:"
Write-Host "  gh release edit $Tag -R $Repo --draft=false"
