<#
.SYNOPSIS
    Install rusty-sched as a Windows Service.

.DESCRIPTION
    Registers `rusty-sched server` as an auto-starting Windows service
    named "RustySchedServer". Run as Administrator.

.PARAMETER BinaryPath
    Full path to rusty-sched.exe. Defaults to "C:\Program Files\rusty-sched\rusty-sched.exe".

.PARAMETER Mode
    Service mode: "server" or "agent". Defaults to "server".

.EXAMPLE
    .\rusty-sched-service.ps1 -Mode server
    .\rusty-sched-service.ps1 -Mode agent
#>

param(
    [string]$BinaryPath = "C:\Program Files\rusty-sched\rusty-sched.exe",
    [ValidateSet("server", "agent")]
    [string]$Mode = "server"
)

if (-not (Test-Path $BinaryPath)) {
    Write-Error "Binary not found at $BinaryPath"
    exit 1
}

$serviceName = if ($Mode -eq "server") { "RustySchedServer" } else { "RustySchedAgent" }
$displayName = if ($Mode -eq "server") { "rusty-sched scheduler server" } else { "rusty-sched execution agent" }

if (Get-Service -Name $serviceName -ErrorAction SilentlyContinue) {
    Write-Host "Service $serviceName already exists. Removing..."
    Stop-Service -Name $serviceName -Force -ErrorAction SilentlyContinue
    sc.exe delete $serviceName | Out-Null
}

Write-Host "Registering Windows service $serviceName ..."
sc.exe create $serviceName `
    binPath= "`"$BinaryPath`" $Mode" `
    DisplayName= "$displayName" `
    start= auto | Out-Null

sc.exe description $serviceName "Reliable job scheduler — see https://github.com/jdp5949/rusty-sched" | Out-Null

Write-Host "Starting service..."
Start-Service -Name $serviceName

Write-Host "Done. Service '$serviceName' is registered and running."
