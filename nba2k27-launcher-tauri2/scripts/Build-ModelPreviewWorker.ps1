param([int]$Jobs = 0)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
& (Join-Path $PSScriptRoot 'Sync-SharedModelPreview.ps1')
$workerRoot = Join-Path $projectRoot 'vendor/model-preview-worker'
$targetRoot = Join-Path $projectRoot 'target-model-preview-worker'
$output = Join-Path $targetRoot 'release/nba2k27-exporter-tauri2.exe'
$inputs = @(
    Get-ChildItem -LiteralPath (Join-Path $workerRoot 'src'),(Join-Path $workerRoot 'shared') -Recurse -File
    Get-Item -LiteralPath (Join-Path $workerRoot 'Cargo.toml'),(Join-Path $workerRoot 'Cargo.lock'),(Join-Path $workerRoot 'build.rs'),(Join-Path $workerRoot 'tauri.conf.json')
)
$latest = ($inputs | Measure-Object -Property LastWriteTimeUtc -Maximum).Maximum
$needsBuild = -not (Test-Path -LiteralPath $output) -or (Get-Item -LiteralPath $output).LastWriteTimeUtc -lt $latest
if ($needsBuild) {
    $cargo = if ($env:CARGO_HOME) {
        Join-Path $env:CARGO_HOME 'bin/cargo.exe'
    } else {
        $bundledCargo = Join-Path (Split-Path $projectRoot -Parent) '.toolchains/rust-exporter/cargo/bin/cargo.exe'
        if (Test-Path -LiteralPath $bundledCargo) {
            $env:CARGO_HOME = Split-Path (Split-Path $bundledCargo -Parent) -Parent
            $env:RUSTUP_HOME = Join-Path (Split-Path $env:CARGO_HOME -Parent) 'rustup'
            $bundledCargo
        } else {
            (Get-Command cargo.exe -ErrorAction SilentlyContinue).Source
        }
    }
    if (-not $cargo -or -not (Test-Path -LiteralPath $cargo)) {
        throw 'The Cargo toolchain is not initialized.'
    }
    $arguments = @('build','--manifest-path',(Join-Path $workerRoot 'Cargo.toml'),'--offline','--locked','--release','--no-default-features','--target-dir',$targetRoot)
    if ($Jobs -gt 0) { $arguments += @('-j', $Jobs) }
    & $cargo @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Model preview worker build failed' }
}
Get-Item -LiteralPath $output
