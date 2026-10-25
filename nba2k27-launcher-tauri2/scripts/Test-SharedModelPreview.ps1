$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$sync = 'D:\Projects\2k Exporter\shared\model-preview\Sync-ModelPreview.ps1'
if (-not (Test-Path -LiteralPath $sync -PathType Leaf)) {
    throw "Canonical Exporter model-preview package is unavailable: $sync"
}
& $sync -LauncherRoot $projectRoot -VerifyOnly
