# Local checks: fmt, clippy and tests for core, windows\ipc and windows\daemon, then the daemon's static-CRT release build.
$ErrorActionPreference = 'Stop'

function Invoke-Step([string]$Dir, [string]$Name, [string[]]$Cargo) {
    Write-Host "==> $Name ($Dir)"
    Push-Location (Join-Path $PSScriptRoot $Dir)
    try { & cargo @Cargo } finally { Pop-Location }
    if ($LASTEXITCODE) { throw "gate: $Name failed in $Dir (exit $LASTEXITCODE)" }
}

foreach ($d in 'core', 'windows\ipc', 'windows\daemon') {
    Invoke-Step $d 'fmt' 'fmt', '--check'
    Invoke-Step $d 'clippy' 'clippy', '--all-targets', '--', '-D', 'warnings'
    Invoke-Step $d 'test' 'test'
}

$saved = $env:RUSTFLAGS
$env:RUSTFLAGS = '-C target-feature=+crt-static'
try { Invoke-Step 'windows\daemon' 'release build' 'build', '--release' }
finally { $env:RUSTFLAGS = $saved }

$exe = Join-Path $PSScriptRoot 'windows\daemon\target\release\podatuid.exe'
Write-Host ("podatuid.exe: {0:N0} bytes" -f (Get-Item $exe).Length)
