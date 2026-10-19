param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
$taskRoot = $PSScriptRoot
$taskSource = Join-Path $taskRoot 'Dev\Pending\QA056\launcher.exe'
$taskExpected = 'E906D3C7F0755BA6A14497AFC2D2DEAD83BA8686F52D6F54C5D09623D45530BC'
function Assert-QA056Idle {
    $taskBusy = @(Get-Process | Where-Object {
        $_.ProcessName -match '^(NBA2K27|2k Launcher.*|2k-launcher-.*|nba2k27-launcher-tauri2|EasyAntiCheat.*)$'
    })
    if ($taskBusy.Count) {
        throw ('Close the game and launcher normally first. Nothing was replaced. Still running: ' +
            (($taskBusy | ForEach-Object { "$($_.ProcessName) (PID $($_.Id))" }) -join ', '))
    }
}
try {
    if ((Get-FileHash -LiteralPath $taskSource).Hash -ne $taskExpected) {
        throw 'QA056 staged dev binary changed; refusing activation.'
    }
    Assert-QA056Idle
    if ($CheckOnly) { Write-Output 'QA056 staged hash verified; game/launcher closed; no activation performed.'; exit 0 }
    $taskDestination = Join-Path $taskRoot 'Dev\Portable\App\2k Launcher Rust Prototype.exe'
    $taskBackup = Join-Path $taskRoot ('Backups\qa056-dev-activation-' + (Get-Date -Format 'yyyyMMdd-HHmmss-ffff'))
    New-Item -ItemType Directory -Path $taskBackup | Out-Null
    Copy-Item -LiteralPath $taskDestination -Destination (Join-Path $taskBackup '2k Launcher Rust Prototype.exe')
    Assert-QA056Idle
    Copy-Item -LiteralPath $taskSource -Destination $taskDestination
    if ((Get-FileHash -LiteralPath $taskDestination).Hash -ne $taskExpected) { throw 'Dev copy verification failed.' }
    $taskViteUp = $false
    try { $taskViteUp = (Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 'http://localhost:5173/').StatusCode -eq 200 } catch {}
    if (-not $taskViteUp) {
        Start-Process -FilePath 'C:\Program Files\nodejs\node.exe' -ArgumentList 'node_modules/vite/bin/vite.js','--host','localhost','--port','5173','--strictPort' -WorkingDirectory $taskRoot -WindowStyle Hidden | Out-Null
    }
    Assert-QA056Idle
    Start-Process -FilePath $taskDestination -WorkingDirectory (Split-Path $taskDestination -Parent) | Out-Null
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show($_.Exception.Message, 'QA056 dev launcher') | Out-Null
    exit 1
}
