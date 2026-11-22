# Build the portable awayd folder for Windows.
#
#   .\packaging\windows\build-portable.ps1            -> dist\awayd-portable (local use / deploy)
#   .\packaging\windows\build-portable.ps1 -Release   -> dist\release\awayd-vX.Y.Z-windows-x64-portable.zip
#                                                        + SHA256SUMS.txt (clean staging folder,
#                                                        third-party licenses, --locked build)
#
# The release staging folder is recreated from scratch, so local runtime data (config, state,
# logs) can never end up in a published ZIP.
param(
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $Root

function Assert-NativeSuccess([string]$What) {
    # $ErrorActionPreference does not apply to native exit codes: without this check a failed
    # command would leave old files in place and the script would report success.
    if ($LASTEXITCODE -ne 0) {
        throw "$What failed (exit code $LASTEXITCODE); portable bundle not updated"
    }
}

if ($Release) {
    cargo build --release --locked
} else {
    cargo build --release
}
Assert-NativeSuccess "cargo build"

$Version = (cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json).packages |
    Where-Object { $_.name -eq "awayd-daemon" } | Select-Object -First 1 -ExpandProperty version
Assert-NativeSuccess "cargo metadata"
if (-not $Version) { throw "could not read the awayd version from cargo metadata" }

if ($Release) {
    $ReleaseDir = Join-Path $Root "dist\release"
    $Name = "awayd-v$Version"
    $Dist = Join-Path $ReleaseDir $Name
    if (Test-Path $ReleaseDir) { Remove-Item -Recurse -Force $ReleaseDir }
} else {
    $Dist = Join-Path $Root "dist\awayd-portable"
}
New-Item -ItemType Directory -Force -Path $Dist, (Join-Path $Dist "config"), (Join-Path $Dist "data\icons") | Out-Null

$IconsSrc = Join-Path $Root "data\icons"
if (Test-Path $IconsSrc) {
    Copy-Item (Join-Path $IconsSrc "*.svg") (Join-Path $Dist "data\icons") -Force
}

$Bins = @("awayd", "awayd-settings", "awayctl")
foreach ($b in $Bins) {
    Copy-Item "target\release\$b.exe" $Dist -Force
}

"" | Out-File (Join-Path $Dist "portable.marker") -Encoding ascii

$Example = @"
schema_version = 1

[schedule]
country = "DE"
subdiv = "BW"

[[schedule.days]]
enabled = true
start = "09:00"
end = "17:00"

[[schedule.days]]
enabled = true
start = "09:00"
end = "17:00"

[[schedule.days]]
enabled = true
start = "09:00"
end = "17:00"

[[schedule.days]]
enabled = true
start = "09:00"
end = "17:00"

[[schedule.days]]
enabled = true
start = "09:00"
end = "17:00"

[[schedule.days]]
enabled = false
start = "09:00"
end = "17:00"

[[schedule.days]]
enabled = false
start = "09:00"
end = "17:00"

[mode]
kind = "schedule"
"@
$Example | Out-File (Join-Path $Dist "config\config.toml.example") -Encoding utf8

foreach ($doc in @("README.md", "LICENSE", "CHANGELOG.md")) {
    Copy-Item (Join-Path $Root $doc) $Dist -Force
}

if (-not $Release) {
    Write-Host "Portable bundle: $Dist"
    return
}

# cargo-about resolves metadata for all platforms and needs every crate of Cargo.lock in the
# local cache (also non-Windows ones); fetch them first, then let it run offline (--frozen).
cargo fetch --locked
Assert-NativeSuccess "cargo fetch"

# Third-party license notices (fails on licenses not accepted in about.toml).
cargo about generate --frozen --fail about.hbs -o (Join-Path $Dist "THIRD_PARTY_LICENSES.html")
Assert-NativeSuccess "cargo about generate"

# cargo-about does not include texts for LicenseRef-* licenses (see about.toml): ship the
# Ubuntu Font Licence of the fonts embedded in awayd-settings.exe from the crate itself.
$Packages = (cargo metadata --format-version 1 --locked --offline | ConvertFrom-Json).packages
Assert-NativeSuccess "cargo metadata"
$Fonts = $Packages | Where-Object { $_.name -eq "epaint_default_fonts" } | Select-Object -First 1
if (-not $Fonts) { throw "epaint_default_fonts not found in cargo metadata" }
$Ufl = Join-Path (Split-Path -Parent $Fonts.manifest_path) "fonts\UFL.txt"
if (-not (Test-Path $Ufl)) { throw "Ubuntu Font Licence text not found: $Ufl" }
Copy-Item $Ufl (Join-Path $Dist "UBUNTU-FONT-LICENCE.txt") -Force

$Zip = Join-Path $ReleaseDir "$Name-windows-x64-portable.zip"
Compress-Archive -Path $Dist -DestinationPath $Zip -CompressionLevel Optimal
$Hash = (Get-FileHash -Algorithm SHA256 $Zip).Hash.ToLowerInvariant()
# sha256sum format ("<hash>  <file>"), LF line endings, no BOM.
[System.IO.File]::WriteAllText((Join-Path $ReleaseDir "SHA256SUMS.txt"), "$Hash  $(Split-Path -Leaf $Zip)`n")

Write-Host "Release bundle: $Zip"
Write-Host "Checksums:      $(Join-Path $ReleaseDir 'SHA256SUMS.txt')"
