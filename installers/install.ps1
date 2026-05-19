<#
.SYNOPSIS
    rusty-sched installer — Windows.

.DESCRIPTION
    Downloads the latest Windows release, extracts rusty-sched.exe, and
    installs it to a per-user bin directory on PATH. No admin required.

.EXAMPLE
    irm https://github.com/jdp5949/rusty-sched/releases/latest/download/install.ps1 | iex
    $env:VERSION='v0.1.0'; irm .../install.ps1 | iex
#>

$ErrorActionPreference = 'Stop'

$Repo = 'jdp5949/rusty-sched'
$Version = if ($env:VERSION) { $env:VERSION } else { $null }
$Prefix = if ($env:PREFIX) { $env:PREFIX } else { Join-Path $HOME '.rusty-sched' }

function Info($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }
function Fail($msg) { Write-Host "error: $msg" -ForegroundColor Red; exit 1 }

if (-not $Version) {
    Info "resolving latest release..."
    try {
        $rel = Invoke-RestMethod -UseBasicParsing "https://api.github.com/repos/$Repo/releases/latest"
        $Version = $rel.tag_name
    } catch {
        Fail "could not resolve latest release: $_"
    }
}
if (-not $Version) { Fail "no version detected" }

$Arch = if ([Environment]::Is64BitOperatingSystem) { 'x86_64-pc-windows-msvc' } else { Fail '32-bit Windows is not supported' }
$Name = "rusty-sched-$Version-$Arch"
$Url = "https://github.com/$Repo/releases/download/$Version/$Name.zip"

$Tmp = Join-Path $env:TEMP "rusty-sched-install-$(Get-Random)"
New-Item -ItemType Directory -Path $Tmp -Force | Out-Null
try {
    $Zip = Join-Path $Tmp "rs.zip"
    Info "downloading $Url"
    Invoke-WebRequest -UseBasicParsing -Uri $Url -OutFile $Zip
    Info "extracting"
    Expand-Archive -Path $Zip -DestinationPath $Tmp -Force

    $Src = Join-Path $Tmp "$Name\rusty-sched.exe"
    if (-not (Test-Path $Src)) { Fail "binary not found: $Src" }

    $Bin = Join-Path $Prefix 'bin'
    New-Item -ItemType Directory -Path $Bin -Force | Out-Null
    $Dest = Join-Path $Bin 'rusty-sched.exe'
    Copy-Item $Src $Dest -Force
    Info "installed: $Dest"

    # Add to user PATH if missing.
    $UserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    if ($UserPath -notlike "*$Bin*") {
        Info "adding $Bin to user PATH"
        [Environment]::SetEnvironmentVariable('PATH', "$UserPath;$Bin", 'User')
    }

    & $Dest version

    Write-Host @"

Next steps:
  rusty-sched server          # boot the scheduler on :8080
  start http://localhost:8080 # web UI

Open a new shell so PATH is picked up.

Docs: https://jdp5949.github.io/rusty-sched/
"@
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
