param([switch]$Desktop)
# Tests + release build. GNU toolchain: puts WinLibs (dlltool, windres) on PATH if installed via winget.
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$winlibs = Get-ChildItem "$env:LOCALAPPDATA\Microsoft\WinGet\Packages" -Directory -Filter 'BrechtSanders.WinLibs*' -ErrorAction SilentlyContinue |
    Select-Object -First 1
if ($winlibs) { $env:PATH = "$($winlibs.FullName)\mingw64\bin;$env:PATH" }

cargo test
if ($LASTEXITCODE) { exit $LASTEXITCODE }
cargo build --release
if ($LASTEXITCODE) { exit $LASTEXITCODE }

$exe = Join-Path $PSScriptRoot 'target\release\dhakaflix-player.exe'
Write-Host "Built $exe"
if ($Desktop) {
    $dest = Join-Path ([Environment]::GetFolderPath('Desktop')) 'DhakaFlix.exe'
    Copy-Item $exe $dest -Force
    Write-Host "Copied to $dest"
}
