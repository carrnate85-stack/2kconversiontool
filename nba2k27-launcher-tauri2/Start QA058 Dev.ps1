param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
$taskRoot = $PSScriptRoot
$taskSource = Join-Path $taskRoot 'Dev\Pending\QA058\launcher.exe'
$taskExpected = '310C0D60C900C27D1E3B5BAC057DE04560688AA209C0C2B74EED066F512DE3C7'
function Assert-QA058Idle {
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
        throw 'QA058 staged dev binary changed; refusing activation.'
    }
    Assert-QA058Idle
    if ($CheckOnly) { Write-Output 'QA058 staged hash verified; game/launcher closed; no activation performed.'; exit 0 }
    $taskDestination = Join-Path $taskRoot 'Dev\Portable\App\2k Launcher Rust Prototype.exe'
    $taskBackup = Join-Path $taskRoot ('Backups\qa058-dev-activation-' + (Get-Date -Format 'yyyyMMdd-HHmmss-ffff'))
    New-Item -ItemType Directory -Path $taskBackup | Out-Null
    Copy-Item -LiteralPath $taskDestination -Destination (Join-Path $taskBackup '2k Launcher Rust Prototype.exe')
    Assert-QA058Idle
    Copy-Item -LiteralPath $taskSource -Destination $taskDestination
    if ((Get-FileHash -LiteralPath $taskDestination).Hash -ne $taskExpected) { throw 'Dev copy verification failed.' }
    $taskViteUp = $false
    try { $taskViteUp = (Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 'http://localhost:5173/').StatusCode -eq 200 } catch {}
    if (-not $taskViteUp) {
        Start-Process -FilePath 'C:\Program Files\nodejs\node.exe' -ArgumentList 'node_modules/vite/bin/vite.js','--host','localhost','--port','5173','--strictPort' -WorkingDirectory $taskRoot -WindowStyle Hidden | Out-Null
    }
    Assert-QA058Idle
    Start-Process -FilePath $taskDestination -WorkingDirectory (Split-Path $taskDestination -Parent) | Out-Null
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show($_.Exception.Message, 'QA058 dev launcher') | Out-Null
    exit 1
}
