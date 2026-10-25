param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$app = Join-Path $projectRoot 'Dev/Portable/App'
$exe = Join-Path $app '2k Launcher Rust Prototype.exe'
$errorLog = Join-Path $projectRoot 'Dev/start-error.log'
try {
    # Opening the installed dev build must never sync sources or invoke Cargo.
    foreach ($required in @($exe, (Join-Path $app 'tools/NBA2K27-ModelPreviewWorker.exe'), (Join-Path $app 'model-preview-current.json'))) {
        if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
            throw "Installed dev build is incomplete: $required. Run Dev.ps1 -NoRun to rebuild."
        }
    }
    $snapshot = Get-Content (Join-Path $app 'model-preview-current.json') -Raw | ConvertFrom-Json
    if ($snapshot.directory -notmatch '^[0-9a-fA-F]{64}-[0-9a-fA-F]{32}$') { throw 'Invalid installed renderer snapshot.' }
    $preview = Join-Path $app ('model-preview/' + $snapshot.directory)
    foreach ($entry in @('index.html', 'bridge.js', 'renderer/model-preview.js')) {
        if (-not (Test-Path -LiteralPath (Join-Path $preview $entry) -PathType Leaf)) { throw "Installed renderer is missing $entry" }
    }
    if ($CheckOnly) {
        Write-Output "Installed dev build ready: $exe (renderer $($snapshot.revision)); no build or Exporter access required."
        exit 0
    }
    $viteUp = $false
    try { $viteUp = (Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 'http://localhost:5173/').StatusCode -eq 200 } catch {}
    if (-not $viteUp) {
        Start-Process -FilePath 'C:/Program Files/nodejs/node.exe' -ArgumentList 'node_modules/vite/bin/vite.js','--host','localhost','--port','5173','--strictPort' -WorkingDirectory $projectRoot -WindowStyle Hidden | Out-Null
        for ($attempt = 0; $attempt -lt 40; $attempt++) {
            Start-Sleep -Milliseconds 250
            try { $viteUp = (Invoke-WebRequest -UseBasicParsing -TimeoutSec 1 'http://localhost:5173/').StatusCode -eq 200 } catch {}
            if ($viteUp) { break }
        }
        if (-not $viteUp) { throw 'The Launcher UI server did not become ready on port 5173.' }
    }
    Start-Process -FilePath $exe -WorkingDirectory $app | Out-Null
    Remove-Item -LiteralPath $errorLog -Force -ErrorAction SilentlyContinue
} catch {
    New-Item -ItemType Directory -Force (Split-Path $errorLog -Parent) | Out-Null
    ($_ | Out-String) | Set-Content -LiteralPath $errorLog -Encoding utf8
    Write-Error $_ -ErrorAction Continue
    exit 1
}
