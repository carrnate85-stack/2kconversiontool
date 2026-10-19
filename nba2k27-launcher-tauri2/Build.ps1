param([switch]$Verify)
$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$workspaceRoot = Split-Path $projectRoot -Parent
$rustRoot = Join-Path $workspaceRoot '.toolchains/rust-exporter'
$env:CARGO_HOME = Join-Path $rustRoot 'cargo'
$env:RUSTUP_HOME = Join-Path $rustRoot 'rustup'
$cargoExecutable = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
Push-Location $projectRoot
try {
    $guardianTarget = Join-Path $projectRoot 'test/startup-video-runtime-offline/target'
    if (Test-Path -LiteralPath $guardianTarget) {
        & (Join-Path $PSScriptRoot 'scripts/Build-StartupAudioGuardian.ps1') -TargetDirectory $guardianTarget
    } else {
        & (Join-Path $PSScriptRoot 'scripts/Build-StartupAudioGuardian.ps1')
    }
    if ($LASTEXITCODE -ne 0) { throw 'Embedded startup audio guardian build failed' }
    & (Join-Path $PSScriptRoot 'scripts/Check-DiagnosticsRace.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Original diagnostics race/prepared-directory regression failed' }
    node scripts/prepare-assets.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Asset preparation failed' }
    node scripts/prepare-outfit-layouts.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Verified outfit layout preparation failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-OutfitOracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Original native outfit policy oracle failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-OutfitPlanOracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Original outfit match plan oracle failed' }
    node scripts/prepare-browser-workspace.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Original browser workspace preparation failed' }
    node scripts/prepare-roster-layout.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Original roster layout preparation failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-RosterOracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Original roster snapshot oracle failed' }
    node scripts/player-write-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Original player-edit validation oracle failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-RosterOracle.ps1') -Kind Write
    if ($LASTEXITCODE -ne 0) { throw 'Original player-write oracle failed' }
    node scripts/team-write-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Original team-edit validation oracle failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-RosterOracle.ps1') -Kind Team
    if ($LASTEXITCODE -ne 0) { throw 'Original team-write oracle failed' }
    node scripts/uniform-write-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Original uniform validation/layout fixtures failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-RosterOracle.ps1') -Kind Uniform
    if ($LASTEXITCODE -ne 0) { throw 'Original uniform-write oracle failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-ProviderOracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Original provider protocol oracle failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-ProviderOracle.ps1') -Kind Assets
    if ($LASTEXITCODE -ne 0) { throw 'Original provider asset oracle failed' }
    node scripts/sigconfig-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Original signature configuration oracle failed' }
    node scripts/selector-generation-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Original selector generation oracle failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-ArenaOracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Original arena plan oracle failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-BallOracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Original ball policy oracle failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-CameraOracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Original camera policy oracle failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-ScalerOracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Original scaler policy oracle failed' }
    & (Join-Path $PSScriptRoot 'scripts/Prepare-FreestyleOracle.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Original Freestyle category policy oracle failed' }
    node scripts/browser-catalog-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Original browser catalog fixtures failed' }
    node scripts/browser-transfer-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Original browser transfer/preset fixtures failed' }
    node scripts/outfit-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Outfit compatibility fixtures failed' }
    node scripts/wardrobe-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Selectable wardrobe compatibility fixtures failed' }
    node scripts/player-dna-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Player DNA compatibility fixtures failed' }
    node scripts/team-dna-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Team DNA compatibility fixtures failed' }
    node scripts/season-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Season merge compatibility fixtures failed' }
    node scripts/season-math-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Season numeric edit compatibility fixtures failed' }
    node scripts/prepare-season-ui-math.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Season UI arithmetic preparation failed' }
    node scripts/prepare-player-web-presentation.mjs
    if ($LASTEXITCODE -ne 0) { throw 'WPF web-editor presentation preparation failed' }
    node scripts/prepare-editor-units.mjs
    if ($LASTEXITCODE -ne 0) { throw 'WPF measurement/color preparation failed' }
    node scripts/reference-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Basketball-Reference parser compatibility fixtures failed' }
    node scripts/rules-regressions.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Verified WPF Game Rules mappings differ' }
    node scripts/parity-audit.mjs
    if ($LASTEXITCODE -ne 0) { throw 'Parity inventory failed' }
    pnpm install --offline --frozen-lockfile
    if ($LASTEXITCODE -ne 0) { throw 'UI dependency restore failed' }
    # Invoke the pinned local tools directly. Package-manager command shims can
    # fail under a host-provided Windows shell even when dependency restore works.
    node node_modules/typescript/bin/tsc --noEmit
    if ($LASTEXITCODE -ne 0) { throw 'TypeScript check failed' }
    node node_modules/vite/bin/vite.js build
    if ($LASTEXITCODE -ne 0) { throw 'TypeScript/frontend build failed' }
    node node_modules/vitest/vitest.mjs run
    if ($LASTEXITCODE -ne 0) { throw 'Frontend tests failed' }
    & $cargoExecutable test --offline --locked -- --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed' }
    & $cargoExecutable build --release --offline --locked -j 1
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $version = (Get-Content -LiteralPath 'package.json' -Raw | ConvertFrom-Json).version
    $portableRoot = Join-Path $projectRoot "Builds/$version/Portable"
    $appRoot = Join-Path $portableRoot 'App'
    New-Item -ItemType Directory -Path $appRoot -Force | Out-Null
    $executable = Join-Path $appRoot '2k Launcher Rust Prototype.exe'
    # Do not overwrite a running prototype or any installed WPF launcher.
    $running = Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $executable }
    if ($running) { throw 'Close this prototype before rebuilding it.' }
    Copy-Item -LiteralPath 'target/release/nba2k27-launcher-tauri2.exe' -Destination $executable
    Copy-Item -LiteralPath 'README.md' -Destination (Join-Path $portableRoot 'README.md')
    Copy-Item -LiteralPath 'QA-REPORT.md' -Destination (Join-Path $portableRoot 'QA-REPORT.md')
    Copy-Item -LiteralPath 'frontend/parity.json' -Destination (Join-Path $portableRoot 'FEATURE-PARITY.json')
    node scripts/prepare-notices.mjs
    if ($LASTEXITCODE -ne 0) { throw 'License collection failed' }
    Copy-Item -LiteralPath 'THIRD-PARTY-NOTICES.txt' -Destination (Join-Path $portableRoot 'THIRD-PARTY-NOTICES.txt')
    if ($Verify) {
        # Compilation is pinned on this machine as a toolchain workaround;
        # real WebView2/overlay checks must use the normal processor set.
        [Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity=[IntPtr]4294967295
        $smokeRoot = Join-Path $projectRoot ('artifacts/smoke-' + [guid]::NewGuid().ToString('N'))
        $process = Start-Process -FilePath $executable -ArgumentList @('--smoke','--smoke-root',('"' + $smokeRoot + '"')) -WindowStyle Hidden -PassThru
        if (-not $process.WaitForExit(60000)) { throw 'UI smoke timeout; inspect this prototype process before stopping it.' }
        $report = Get-Content -LiteralPath (Join-Path $smokeRoot 'Logs/ui-smoke.json') -Raw | ConvertFrom-Json
        if ($process.ExitCode -ne 0 -or -not $report.result.pass) { throw "UI smoke failed: $($report.result.error)" }
        node scripts/verify-shared-exports.mjs $smokeRoot
        if ($LASTEXITCODE -ne 0) { throw 'WPF shared-file compatibility check failed' }
        $report | ConvertTo-Json -Depth 5
    }
    $buildInfo = [ordered]@{
        version = $version
        builtUtc = [DateTime]::UtcNow.ToString('o')
        sha256 = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
        bytes = (Get-Item -LiteralPath $executable).Length
        isolatedUiVerified = [bool]$Verify
        uiChecks = $(if ($Verify) { $report.result.checks.Count } else { 0 })
        parityComplete = $false
        mainInstalled = $false
    }
    $buildInfo | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $portableRoot 'BUILD-INFO.json') -Encoding utf8
    Get-Item -LiteralPath $executable | Select-Object FullName,Length
} finally { Pop-Location }
