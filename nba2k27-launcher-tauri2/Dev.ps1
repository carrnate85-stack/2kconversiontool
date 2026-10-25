# Fast dev loop (no release build):
#   UI (CSS/TSX/interactions): Vite hot reload inside the running app.
#   Explicit rebuild/update only. Start Dev.vbs opens the last installed build.
# The app runs from Dev\Portable (its own Data/Logs), never a release Builds folder.
# Close any other 2k launcher first: the outfit worker refuses to run while
# another launcher-named process exists.
param([switch]$NoRun, [switch]$ConservativeCompiler)
$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$rustRoot = Join-Path (Split-Path $projectRoot -Parent) '.toolchains/rust-exporter'
$env:CARGO_HOME = Join-Path $rustRoot 'cargo'
$env:RUSTUP_HOME = Join-Path $rustRoot 'rustup'
$cargo = Join-Path $env:CARGO_HOME 'bin/cargo.exe'
$node = 'C:\Program Files\nodejs\node.exe'
$startupErrorLog = Join-Path $projectRoot 'Dev\start-error.log'
trap {
    New-Item -ItemType Directory -Force (Split-Path $startupErrorLog -Parent) | Out-Null
    ($_ | Out-String) | Set-Content -LiteralPath $startupErrorLog -Encoding utf8
    Write-Error $_
    exit 1
}
Push-Location $projectRoot
try {
    # Sync and verify the canonical Exporter renderer before either Vite or the
    # native worker can consume the generated standalone Launcher mirrors.
    $jobs = if ($ConservativeCompiler) { 1 } else { 0 }
    & (Join-Path $projectRoot 'scripts/Build-ModelPreviewWorker.ps1') -Jobs $jobs
    # Vite dev server (hot reload), started once and reused.
    $viteUp = $false
    try { $viteUp = (Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 'http://localhost:5173/').StatusCode -eq 200 } catch {}
    if (-not $viteUp) {
        Start-Process -FilePath $node -ArgumentList 'node_modules/vite/bin/vite.js','--host','localhost','--port','5173','--strictPort' -WorkingDirectory $projectRoot -WindowStyle Hidden | Out-Null
    }
    # Use Cargo's available parallelism by default. The single-job mode remains
    # an explicit fallback for diagnosing a compiler failure.
    $buildArguments = @('build','--offline','--locked','--bin','nba2k27-launcher-tauri2','--no-default-features','--features','dev-model-preview','--target-dir','target-qa054')
    if ($jobs -gt 0) { $buildArguments += @('-j', $jobs) }
    $build = Start-Process -FilePath $cargo -ArgumentList $buildArguments -WorkingDirectory $projectRoot -NoNewWindow -PassThru
    $null = $build.Handle  # without a cached handle ExitCode reads back empty
    if ($ConservativeCompiler) { $build.ProcessorAffinity = [IntPtr]4278190080 }
    $build.WaitForExit()
    if ($build.ExitCode -ne 0) { throw 'Debug build failed' }
    $app = Join-Path $projectRoot 'Dev/Portable/App'
    New-Item -ItemType Directory -Force $app | Out-Null
    $exe = Join-Path $app '2k Launcher Rust Prototype.exe'
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target-qa054/debug/nba2k27-launcher-tauri2.exe') -Destination $exe -Force
    $previewTools = Join-Path $app 'tools'
    New-Item -ItemType Directory -Force $previewTools | Out-Null
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target-model-preview-worker/release/nba2k27-exporter-tauri2.exe') -Destination (Join-Path $previewTools 'NBA2K27-ModelPreviewWorker.exe') -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'vendor/model-preview-worker/THIRD-PARTY-NOTICES.txt') -Destination (Join-Path $previewTools 'MODEL-PREVIEW-THIRD-PARTY-NOTICES.txt') -Force
    Copy-Item -LiteralPath (Join-Path $projectRoot 'dev-assets/model-preview/PREVIEW-REVISION.json') -Destination (Join-Path $previewTools 'MODEL-PREVIEW-REVISION.json') -Force
    & (Join-Path $projectRoot 'scripts/Stage-DevModelPreview.ps1')
    Remove-Item -LiteralPath $startupErrorLog -Force -ErrorAction SilentlyContinue
    if (-not $NoRun) { Start-Process -FilePath $exe -WorkingDirectory $app | Out-Null }
    "Dev app: $exe (UI hot reloads from http://localhost:5173; release-intended 3D preview staged for testing)"
} finally {
    Pop-Location
}
