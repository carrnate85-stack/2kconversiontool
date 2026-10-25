param([switch]$VerifyOnly)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$canonicalRoot = if ($env:NBA2K_MODEL_PREVIEW_ROOT) {
    [System.IO.Path]::GetFullPath($env:NBA2K_MODEL_PREVIEW_ROOT)
} else {
    'D:\Projects\2k Exporter\shared\model-preview'
}
$canonicalManifest = Join-Path $canonicalRoot 'revision.json'
$canonicalSync = Join-Path $canonicalRoot 'Sync-ModelPreview.ps1'
$launcherManifest = Join-Path $projectRoot 'dev-assets\model-preview\PREVIEW-REVISION.json'

if (-not (Test-Path -LiteralPath $canonicalSync -PathType Leaf)) {
    throw "Canonical Exporter model-preview package is unavailable: $canonicalSync"
}
if (-not (Test-Path -LiteralPath $canonicalManifest -PathType Leaf)) {
    throw "Canonical model-preview revision manifest is unavailable: $canonicalManifest"
}
$sourceRevision = (Get-Content -LiteralPath $canonicalManifest -Raw | ConvertFrom-Json).revision
if ($sourceRevision -notmatch '^[0-9A-Fa-f]{64}$') {
    throw "Canonical model-preview revision manifest has an invalid revision: $canonicalManifest"
}

if ($VerifyOnly) {
    & $canonicalSync -LauncherRoot $projectRoot -VerifyOnly
} else {
    & $canonicalSync -LauncherRoot $projectRoot
}

$launcherRevision = (Get-Content -LiteralPath $launcherManifest -Raw | ConvertFrom-Json).revision
if ($launcherRevision -ne $sourceRevision) {
    throw 'Canonical model-preview changed during synchronization. Wait for the Exporter update to finish, then start the dev launcher again.'
}
# Development follows the published canonical revision. Verify the complete
# mirror after syncing, and record that revision in the staged runtime manifest.
if (-not $VerifyOnly) {
    & $canonicalSync -LauncherRoot $projectRoot -VerifyOnly
}
# The preview Rust sources call the shared codec. Keep that build dependency
# in step too; the worker owns only its thin host adapter, never a codec fork.
$assetsSource = Join-Path (Split-Path $canonicalRoot -Parent) 'nba2k-assets'
$assetsDestination = Join-Path $projectRoot 'vendor/model-preview-worker/shared/nba2k-assets'
$assetFiles = @(
    Get-Item -LiteralPath (Join-Path $assetsSource 'Cargo.toml')
    Get-ChildItem -LiteralPath (Join-Path $assetsSource 'src') -File -Recurse
)
foreach ($file in $assetFiles) {
    $relative = $file.FullName.Substring($assetsSource.Length + 1)
    $destination = Join-Path $assetsDestination $relative
    $expectedHash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash
    if (-not $VerifyOnly) {
        New-Item -ItemType Directory -Path (Split-Path $destination -Parent) -Force | Out-Null
        Copy-Item -LiteralPath $file.FullName -Destination $destination -Force
    }
    if (-not (Test-Path -LiteralPath $destination -PathType Leaf) -or
        (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash -ne $expectedHash) {
        throw "Shared preview codec drift detected: $relative"
    }
}
Write-Output "Launcher shared model preview verified at canonical revision $launcherRevision."
