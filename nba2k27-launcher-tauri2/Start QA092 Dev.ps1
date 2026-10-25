param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
$taskRoot = $PSScriptRoot
$taskSource = Join-Path $taskRoot 'Dev\Pending\QA092\launcher.exe'
$taskExpected = 'FE13706533890987BA78FE163E0D4B2CAECEE9EB60DDFD0061EFCB3430882E1E'
$taskExpectedBytes = 46669312
function Assert-QA092Idle {
    $taskBusy = @(Get-Process | Where-Object {
        $_.ProcessName -match '^(NBA2K27|2k Launcher.*|2k-launcher-.*|nba2k27-launcher-tauri2|EasyAntiCheat.*)$'
    })
    if ($taskBusy.Count) {
        throw ('Close the game and launcher normally first. Nothing was replaced. Still running: ' +
            (($taskBusy | ForEach-Object { "$($_.ProcessName) (PID $($_.Id))" }) -join ', '))
    }
}
try {
    if ((Get-Item -LiteralPath $taskSource).Length -ne $taskExpectedBytes) {throw 'QA092 staged size changed; refusing activation.'}
    if ((Get-FileHash -LiteralPath $taskSource).Hash -ne $taskExpected) {
        throw 'QA092 staged dev binary changed; refusing activation.'
    }
    Assert-QA092Idle
    if ($CheckOnly) { Write-Output 'QA092 staged hash verified; game/launcher closed; no activation performed.'; exit 0 }
    $taskDestination = Join-Path $taskRoot 'Dev\Portable\App\2k Launcher Rust Prototype.exe'
    $taskBackup = Join-Path $taskRoot ('Backups\qa092-dev-activation-' + (Get-Date -Format 'yyyyMMdd-HHmmss-ffff'))
    New-Item -ItemType Directory -Path $taskBackup | Out-Null
    Copy-Item -LiteralPath $taskDestination -Destination (Join-Path $taskBackup '2k Launcher Rust Prototype.exe')
    Assert-QA092Idle
    Copy-Item -LiteralPath $taskSource -Destination $taskDestination
    $taskFlush = [IO.File]::Open($taskDestination,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read)
    try { $taskFlush.Flush($true) } finally { $taskFlush.Dispose() }
    if ((Get-FileHash -LiteralPath $taskDestination).Hash -ne $taskExpected) { throw 'Dev copy verification failed.' }
    $taskViteUp = $false
    try { $taskViteUp = (Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 'http://localhost:5173/').StatusCode -eq 200 } catch {}
    if (-not $taskViteUp) {
        Start-Process -FilePath 'C:\Program Files\nodejs\node.exe' -ArgumentList 'node_modules/vite/bin/vite.js','--host','localhost','--port','5173','--strictPort' -WorkingDirectory $taskRoot -WindowStyle Hidden | Out-Null
    }
    Assert-QA092Idle
    $taskPreviousShadow = [Environment]::GetEnvironmentVariable('C27_OUTFIT_READY_SHADOW','Process')
    try {
        $env:C27_OUTFIT_READY_SHADOW = '1'
        Start-Process -FilePath $taskDestination -WorkingDirectory (Split-Path $taskDestination -Parent) | Out-Null
    } finally { [Environment]::SetEnvironmentVariable('C27_OUTFIT_READY_SHADOW',$taskPreviousShadow,'Process') }
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show($_.Exception.Message, 'QA092 dev launcher') | Out-Null
    exit 1
}
