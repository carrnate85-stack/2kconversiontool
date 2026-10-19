# Fast dev loop (no release build):
#   UI (CSS/TSX/interactions): Vite hot reload inside the running app.
#   Rust: incremental debug build; rerun this script after Rust edits.
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
Push-Location $projectRoot
try {
    # Vite dev server (hot reload), started once and reused.
    $viteUp = $false
    try { $viteUp = (Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 'http://localhost:5173/').StatusCode -eq 200 } catch {}
    if (-not $viteUp) {
        Start-Process -FilePath $node -ArgumentList 'node_modules/vite/bin/vite.js','--host','localhost','--port','5173','--strictPort' -WorkingDirectory $projectRoot -WindowStyle Hidden | Out-Null
    }
    # Isolated compiler tests passed unrestricted affinity/-j8. Keep the old
    # restricted setting as an explicit fallback, not a claim about root cause.
    $jobs = if ($ConservativeCompiler) { '1' } else { '8' }
    $build = Start-Process -FilePath $cargo -ArgumentList 'build','--offline','--locked','--no-default-features','--target-dir','target-qa054','-j',$jobs -WorkingDirectory $projectRoot -NoNewWindow -PassThru
    $null = $build.Handle  # without a cached handle ExitCode reads back empty
    if ($ConservativeCompiler) { $build.ProcessorAffinity = [IntPtr]4278190080 }
    $build.WaitForExit()
    if ($build.ExitCode -ne 0) { throw 'Debug build failed' }
    $app = Join-Path $projectRoot 'Dev/Portable/App'
    New-Item -ItemType Directory -Force $app | Out-Null
    $exe = Join-Path $app '2k Launcher Rust Prototype.exe'
    Copy-Item -LiteralPath (Join-Path $projectRoot 'target-qa054/debug/nba2k27-launcher-tauri2.exe') -Destination $exe -Force
    if (-not $NoRun) { Start-Process -FilePath $exe -WorkingDirectory $app | Out-Null }
    "Dev app: $exe (UI hot reloads from http://localhost:5173)"
} finally {
    Pop-Location
}
