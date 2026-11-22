# Stop running awayd portable processes, then deploy a fresh build to the live folder.
#
# Updates: *.exe, data\, portable.marker, config\config.toml.example
# Preserves: config\config.toml and state\ (your live settings, tokens, logs)
#
# Default paths (relative to this script):
#   Source:      ../../dist/awayd-portable
#   Destination: ../../../awayd-portable  (sibling of the repository folder)
#
# Usage (from anywhere):
#   .\packaging\windows\deploy-portable.ps1
#
# Optional: -Build to build first; -Destination to override the live folder.

param(
    [switch]$Build,
    [string]$Source,
    [string]$Destination,
    [int]$StopWaitSeconds = 8
)

$ErrorActionPreference = "Stop"

# packaging/windows -> repository root
$AwaydRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
# parent of the repository folder (live bundle is a sibling: <parent>\awayd-portable)
$AppsRoot = Split-Path -Parent $AwaydRoot

if (-not $Source) {
    $Source = Join-Path $AwaydRoot "dist\awayd-portable"
}
if (-not $Destination) {
    $Destination = Join-Path $AppsRoot "awayd-portable"
}

$ProcessNames = @("awayd", "awayd-settings")

function Stop-AwaydProcesses {
    param([string[]]$Names, [int]$WaitSeconds)

    $stopped = @()
    foreach ($name in $Names) {
        $procs = Get-Process -Name $name -ErrorAction SilentlyContinue
        if (-not $procs) {
            continue
        }
        foreach ($proc in $procs) {
            Write-Host "Stopping $($proc.ProcessName) (PID $($proc.Id))..."
            Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
            $stopped += $proc.ProcessName
        }
    }

    if ($stopped.Count -eq 0) {
        Write-Host "No awayd processes were running."
        return
    }

    $deadline = (Get-Date).AddSeconds($WaitSeconds)
    while ((Get-Date) -lt $deadline) {
        $still = @()
        foreach ($name in ($stopped | Select-Object -Unique)) {
            if (Get-Process -Name $name -ErrorAction SilentlyContinue) {
                $still += $name
            }
        }
        if ($still.Count -eq 0) {
            Write-Host "Processes exited."
            return
        }
        Start-Sleep -Milliseconds 250
    }

    $remaining = @()
    foreach ($name in $ProcessNames) {
        if (Get-Process -Name $name -ErrorAction SilentlyContinue) {
            $remaining += $name
        }
    }
    if ($remaining.Count -gt 0) {
        throw "Timed out waiting for: $($remaining -join ', '). Close them manually and rerun."
    }
}

function Deploy-PortableBundle {
    param([string]$Src, [string]$Dst)

    if (-not (Test-Path -LiteralPath $Src)) {
        throw "Source not found: $Src`nRun build-portable.ps1 first (or pass -Build)."
    }

    if (-not (Test-Path -LiteralPath $Dst)) {
        Write-Host "Creating destination: $Dst"
        New-Item -ItemType Directory -Force -Path $Dst | Out-Null
    }

    New-Item -ItemType Directory -Force -Path (Join-Path $Dst "config") | Out-Null

    Write-Host "Copying executables..."
    Get-ChildItem -LiteralPath $Src -Filter "*.exe" -File | ForEach-Object {
        Copy-Item -LiteralPath $_.FullName -Destination $Dst -Force
        Write-Host "  $($_.Name)"
    }

    $marker = Join-Path $Src "portable.marker"
    if (Test-Path -LiteralPath $marker) {
        Copy-Item -LiteralPath $marker -Destination $Dst -Force
    }

    $dataSrc = Join-Path $Src "data"
    if (Test-Path -LiteralPath $dataSrc) {
        Write-Host "Copying data\..."
        Copy-Item -LiteralPath $dataSrc -Destination $Dst -Recurse -Force
    }

    $exampleSrc = Join-Path $Src "config\config.toml.example"
    if (Test-Path -LiteralPath $exampleSrc) {
        Copy-Item -LiteralPath $exampleSrc -Destination (Join-Path $Dst "config") -Force
    }

    Write-Host ""
    Write-Host "Deployed to: $Dst"
    Write-Host "Kept unchanged: config\config.toml, state\"
}

Write-Host "Source:      $Source"
Write-Host "Destination: $Destination"
Write-Host ""

# Stop before build too — awayd may be running from dist or the live folder and lock *.exe.
Stop-AwaydProcesses -Names $ProcessNames -WaitSeconds $StopWaitSeconds

if ($Build) {
    $buildScript = Join-Path $PSScriptRoot "build-portable.ps1"
    if (-not (Test-Path -LiteralPath $buildScript)) {
        throw "Build script not found: $buildScript"
    }
    Write-Host "Building portable bundle..."
    & $buildScript
    $Source = Join-Path $AwaydRoot "dist\awayd-portable"
}

Deploy-PortableBundle -Src $Source -Dst $Destination

Write-Host ""
Write-Host "Done. Start awayd.exe from the destination folder when ready."
