$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot -Parent
$source = Join-Path $projectRoot 'dev-assets/model-preview'
$app = Join-Path $projectRoot 'Dev/Portable/App'
$manifest = Get-Content (Join-Path $source 'PREVIEW-REVISION.json') -Raw | ConvertFrom-Json
if ($manifest.revision -notmatch '^[0-9a-fA-F]{64}$') { throw 'Invalid renderer revision.' }
$name = $manifest.revision + '-' + [Guid]::NewGuid().ToString('N')
$destination = Join-Path $app ('model-preview/' + $name)
New-Item -ItemType Directory -Path $destination -Force | Out-Null
Get-ChildItem -LiteralPath $source | Copy-Item -Destination $destination -Recurse -Force
foreach ($file in $manifest.files.PSObject.Properties) {
    if (-not $file.Name.StartsWith('renderer/')) { continue }
    $actual = (Get-FileHash -LiteralPath (Join-Path $destination $file.Name) -Algorithm SHA256).Hash
    if ($actual -ne $file.Value) { throw "Renderer changed while staging: $($file.Name). Installed snapshot remains active." }
}
# Activate only a complete local snapshot. The live Exporter and generated
# build mirrors may keep changing without changing what the dev app serves.
$pointer = Join-Path $app 'model-preview-current.json'
$pending = Join-Path $app ('model-preview-current-' + [Guid]::NewGuid().ToString('N') + '.json')
[ordered]@{ directory = $name; revision = $manifest.revision } | ConvertTo-Json | Set-Content -LiteralPath $pending -Encoding utf8
if (Test-Path -LiteralPath $pointer) {
    [IO.File]::Replace($pending, $pointer, $null)
} else {
    [IO.File]::Move($pending, $pointer)
}
Write-Output "Installed renderer snapshot: $($manifest.revision)"
