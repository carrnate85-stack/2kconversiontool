param([string]$TargetDirectory = '')
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$workspaceRoot = Split-Path $projectRoot -Parent
$cargoPath = Join-Path $workspaceRoot '.toolchains/rust-exporter/cargo'
$env:CARGO_HOME = $cargoPath
$env:RUSTUP_HOME = Join-Path $workspaceRoot '.toolchains/rust-exporter/rustup'
if (-not $TargetDirectory) { $TargetDirectory = Join-Path $projectRoot 'target' }
$targetPath = [IO.Path]::GetFullPath($TargetDirectory)
$projectPath = [IO.Path]::GetFullPath($projectRoot).TrimEnd('\') + '\'
if (-not $targetPath.StartsWith($projectPath, [StringComparison]::OrdinalIgnoreCase)) {
  throw 'Startup guardian build target must stay inside this workspace project.'
}
$manifest = Join-Path $projectRoot 'native/startup-audio-guardian/Cargo.toml'
& (Join-Path $cargoPath 'bin/cargo.exe') build --manifest-path $manifest --target-dir $targetPath --release --offline --locked
if ($LASTEXITCODE -ne 0) { throw 'Audio-only Rust guardian build failed.' }
$binary = Join-Path $targetPath 'release/startup-audio-guardian.exe'
$asset = Join-Path $projectRoot 'assets/startup-audio-guardian.exe'
$sha = (Get-FileHash -LiteralPath $binary -Algorithm SHA256).Hash.ToLowerInvariant()
Copy-Item -LiteralPath $binary -Destination $asset -Force
# These are generated, project-owned build assets. No installed App, portable
# Data/Library, game files, AppData, registry, process or audio calls are made.
[IO.File]::WriteAllText((Join-Path $projectRoot 'assets/startup-audio-guardian.sha256'), $sha, [Text.Encoding]::ASCII)
[pscustomobject]@{binary=$asset;sha256=$sha;bytes=(Get-Item -LiteralPath $asset).Length;audioOnly=$true;gameAccess=$false} | ConvertTo-Json -Compress
