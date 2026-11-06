<#
    Custom actions for podatui.msi. The MSI runs this elevated, as SYSTEM, from the
    install folder, and passes who is installing (their SID and profile folders).

      -Action install         take over older installs, register the helper tasks,
                              and with -Drivers also test-sign + install both drivers
                              (-MicDriver acx|portcls, from the MSI's mic choice)
      -Action remove-drivers  remove both drivers and the test certificate
      -Action uninstall       stop podatui, remove the helper tasks and startup entries
      -Action stop            stop podatui so its files can be replaced

    Output goes to the MSI log and to %ProgramData%\podatui\setup.log.
#>
param(
    [Parameter(Mandatory)][ValidateSet('install', 'remove-drivers', 'uninstall', 'stop')][string]$Action,
    [string]$UserSid,
    [string]$LocalAppData,
    [string]$AppData,
    [switch]$Drivers,
    [ValidateSet('auto', 'acx', 'portcls')][string]$MicDriver = 'auto'
)
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'setup-common.ps1')

$logDir = Join-Path $env:ProgramData 'podatui'
New-Item -ItemType Directory -Force -Path $logDir | Out-Null
Start-Transcript -Append -Path (Join-Path $logDir 'setup.log') | Out-Null
try {
    Write-Host "==> msi-setup $Action (user $UserSid, drivers: $Drivers, mic: $MicDriver)"
    switch ($Action) {
        'stop' {
            Stop-Podatui
        }
        'install' {
            Stop-Podatui
            Remove-UpstreamNTPods $UserSid $LocalAppData $AppData
            Remove-LegacyLibrePods $UserSid $LocalAppData $AppData
            Remove-ZipInstall $LocalAppData $AppData
            Remove-SetupLeftovers $LocalAppData
            if ($Drivers) {
                if (-not (Test-TestMode)) { throw 'Test Mode is not active; the drivers would not load.' }
                $work = Join-Path $env:ProgramData "podatui\drivers-$(Get-Random)"
                try { Install-PodatuiDrivers $PSScriptRoot $work $MicDriver }
                finally { Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue }
            }
            Register-PodatuiTasks $PSScriptRoot $UserSid
            New-Item -ItemType Directory -Force -Path (Join-Path $LocalAppData 'podatui') | Out-Null
        }
        'remove-drivers' {
            Stop-Podatui
            Uninstall-PodatuiDrivers $PSScriptRoot
        }
        'uninstall' {
            Stop-Podatui
            Unregister-PodatuiTasks
            # The app's own "Start with Windows" setting writes these too.
            Remove-RunValues $UserSid 'podatui', 'podatui Daemon'
        }
    }
    Write-Host "==> msi-setup $Action done"
    exit 0
} catch {
    Write-Host "ERROR: $_"
    Write-Host $_.ScriptStackTrace
    exit 1
} finally {
    Stop-Transcript | Out-Null
}
