# Build portable awayd folder for Windows 11
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
Set-Location $Root

cargo build --release

$Dist = Join-Path $Root "dist\awayd-portable"
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

Write-Host "Portable bundle: $Dist"
