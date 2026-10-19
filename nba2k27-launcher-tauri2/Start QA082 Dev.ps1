param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
$taskRoot = $PSScriptRoot
$taskSource = Join-Path $taskRoot 'Dev\Pending\QA082\launcher.exe'
$taskExpected = '71404BF01756E4A63ED267BCF4860CD023BA8E0FCA1FBF84813E4646F63D882D'
$taskExpectedBytes = 53273600
function Assert-QA082Idle {
    $taskBusy = @(Get-Process | Where-Object {
        $_.ProcessName -match '^(NBA2K27|2k Launcher.*|2k-launcher-.*|nba2k27-launcher-tauri2|EasyAntiCheat.*)$'
    })
    if ($taskBusy.Count) {
        throw ('Close the game and launcher normally first. Nothing was replaced. Still running: ' +
            (($taskBusy | ForEach-Object { "$($_.ProcessName) (PID $($_.Id))" }) -join ', '))
    }
}
try {
    if ($taskExpected -eq ('0'*64) -or $taskExpectedBytes -le 0) {throw 'QA082 final hashes and size are pending; activation is disabled.'}
    if ((Get-Item -LiteralPath $taskSource).Length -ne $taskExpectedBytes) {throw 'QA082 staged size changed; refusing activation.'}
    if ((Get-FileHash -LiteralPath $taskSource).Hash -ne $taskExpected) {
        throw 'QA082 staged dev binary changed; refusing activation.'
    }
    Assert-QA082Idle
    if ($CheckOnly) { Write-Output 'QA082 staged hash verified; game/launcher closed; no activation performed.'; exit 0 }
    $taskDestination = Join-Path $taskRoot 'Dev\Portable\App\2k Launcher Rust Prototype.exe'
    $taskBackup = Join-Path $taskRoot ('Backups\qa082-dev-activation-' + (Get-Date -Format 'yyyyMMdd-HHmmss-ffff'))
    New-Item -ItemType Directory -Path $taskBackup | Out-Null
    Copy-Item -LiteralPath $taskDestination -Destination (Join-Path $taskBackup '2k Launcher Rust Prototype.exe')
    Assert-QA082Idle
    Copy-Item -LiteralPath $taskSource -Destination $taskDestination
    $taskFlush = [IO.File]::Open($taskDestination,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read)
    try { $taskFlush.Flush($true) } finally { $taskFlush.Dispose() }
    if ((Get-FileHash -LiteralPath $taskDestination).Hash -ne $taskExpected) { throw 'Dev copy verification failed.' }
    $taskViteUp = $false
    try { $taskViteUp = (Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 'http://localhost:5173/').StatusCode -eq 200 } catch {}
    if (-not $taskViteUp) {
        Start-Process -FilePath 'C:\Program Files\nodejs\node.exe' -ArgumentList 'node_modules/vite/bin/vite.js','--host','localhost','--port','5173','--strictPort' -WorkingDirectory $taskRoot -WindowStyle Hidden | Out-Null
    }
    Assert-QA082Idle
    # Dev-only read-only timing observer; the normal provider child inherits this.
    $taskPreviousShadow = [Environment]::GetEnvironmentVariable('C27_OUTFIT_READY_SHADOW','Process')
    try {
        $env:C27_OUTFIT_READY_SHADOW = '1'
    Start-Process -FilePath $taskDestination -WorkingDirectory (Split-Path $taskDestination -Parent) | Out-Null
    } finally { [Environment]::SetEnvironmentVariable('C27_OUTFIT_READY_SHADOW',$taskPreviousShadow,'Process') }
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show($_.Exception.Message, 'QA082 dev launcher') | Out-Null
    exit 1
}
