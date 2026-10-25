# Developer check only. Never opens the game, syncs Exporter, or installs a build.
param(
    [string]$GameExe = 'C:/Program Files (x86)/Steam/steamapps/common/NBA 2K27/NBA2K27.exe',
    [string]$Python = "$env:USERPROFILE/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe"
)
$ErrorActionPreference = 'Stop'
if (-not (Test-Path -LiteralPath $GameExe -PathType Leaf)) { throw "Game executable not found: $GameExe" }
$projectRoot = Split-Path $PSScriptRoot -Parent
$rustRoot = Join-Path (Split-Path $projectRoot -Parent) '.toolchains/rust-exporter'
$env:CARGO_HOME = Join-Path $rustRoot 'cargo'
$env:RUSTUP_HOME = Join-Path $rustRoot 'rustup'
$previousGameExe = $env:C27_ROSTER_GAME_EXE
$previousOutfitExe = $env:C27_OUTFIT_DISK_AUDIT
$env:C27_ROSTER_GAME_EXE = (Resolve-Path -LiteralPath $GameExe).Path
$env:C27_OUTFIT_DISK_AUDIT = $env:C27_ROSTER_GAME_EXE
$taskOutput = Join-Path $projectRoot 'artifacts/roster-profile-audit'
New-Item -ItemType Directory -Force -Path $taskOutput | Out-Null
$taskFailures = [System.Collections.Generic.List[string]]::new()
function Invoke-ProfileCheck([string]$Name, [scriptblock]$Check) {
    Write-Host "Checking $Name"
    try {
        & $Check *> (Join-Path $taskOutput "$Name.log")
        if ($LASTEXITCODE -ne 0) { throw "Exit code $LASTEXITCODE" }
        Write-Host "PASS: $Name"
    } catch {
        $taskFailures.Add("${Name}: $_")
        Write-Host "FAIL: $Name (see its log)"
    }
}
Push-Location $projectRoot
try {
    $taskCargo = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
    $taskCargoArgs = @('test','--offline','--locked','--bin','nba2k27-launcher-tauri2','--no-default-features','--features','dev-model-preview','--target-dir','target-qa054')
    Invoke-ProfileCheck 'rust-all-profiles-and-runtime-states' {
        & $taskCargo @taskCargoArgs exact_version_two_write_profile_is_verified_from_disk_only -- --ignored --nocapture
    }
    Invoke-ProfileCheck 'native-provider-and-embedded-dll' {
        & $taskCargo @taskCargoArgs provider_build_profile::outfit::tests::exact_installed_disk_build -- --ignored --nocapture
    }
    Invoke-ProfileCheck 'roster-fingerprint-inventory' {
        & $Python tools/audit_roster_current.py $env:C27_ROSTER_GAME_EXE --output (Join-Path $taskOutput 'fingerprints.json')
    }
    Invoke-ProfileCheck 'native-measurement-evidence-parity' {
        & $Python tools/check_native_appearance_profile.py $env:C27_ROSTER_GAME_EXE
    }
    Invoke-ProfileCheck 'native-runtime-validators' {
        $taskCompiler = Join-Path (Split-Path $projectRoot -Parent) '.toolchains/llvm-mingw-20260826-ucrt-x86_64/bin/x86_64-w64-mingw32-clang++.exe'
        $taskFixture = Join-Path $taskOutput 'roster-native-profile-disk-test.exe'
        & $taskCompiler --std=c++20 -O2 -Wall -Wextra -Werror -static -D_WIN32_WINNT=0x0602 -DC27_APPEARANCE_SEH=1 -municode test/roster-native-profile-disk-test.cpp -lbcrypt -ladvapi32 -o $taskFixture
        if ($LASTEXITCODE -ne 0) { throw 'Native audit fixture compile failed' }
        & $taskFixture $env:C27_ROSTER_GAME_EXE
    }
    if ($taskFailures.Count) { throw "Roster audit failures:`n$($taskFailures -join "`n")`nLogs: $taskOutput" }
    Write-Host "All roster version checks passed. Logs: $taskOutput"
} finally {
    $env:C27_ROSTER_GAME_EXE = $previousGameExe
    $env:C27_OUTFIT_DISK_AUDIT = $previousOutfitExe
    Pop-Location
}
