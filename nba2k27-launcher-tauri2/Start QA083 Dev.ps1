param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
$taskRoot = $PSScriptRoot
$taskSource = Join-Path $taskRoot 'Dev\Pending\QA083\launcher.exe'
$taskExpected = '5040AA978A29AD809DDA889EF97D2B68FEA7AEBB3D5531FEF81DFD8FFE2CB765'
$taskExpectedBytes = 55046656
function Assert-QA083Idle {
    $taskBusy = @(Get-Process | Where-Object {
        $_.ProcessName -match '^(NBA2K27|2k Launcher.*|2k-launcher-.*|nba2k27-launcher-tauri2|EasyAntiCheat.*)$'
    })
    if ($taskBusy.Count) {
        throw ('Close the game and launcher normally first. Nothing was replaced. Still running: ' +
            (($taskBusy | ForEach-Object { "$($_.ProcessName) (PID $($_.Id))" }) -join ', '))
    }
}
try {
    if ($taskExpected -eq ('0'*64) -or $taskExpectedBytes -le 0) {throw 'QA083 final hashes and size are pending; activation is disabled.'}
    if ((Get-Item -LiteralPath $taskSource).Length -ne $taskExpectedBytes) {throw 'QA083 staged size changed; refusing activation.'}
    if ((Get-FileHash -LiteralPath $taskSource).Hash -ne $taskExpected) {
        throw 'QA083 staged dev binary changed; refusing activation.'
    }
    Assert-QA083Idle
    if ($CheckOnly) { Write-Output 'QA083 staged hash verified; game/launcher closed; no activation performed.'; exit 0 }
    $taskDestination = Join-Path $taskRoot 'Dev\Portable\App\2k Launcher Rust Prototype.exe'
    $taskBackup = Join-Path $taskRoot ('Backups\qa083-dev-activation-' + (Get-Date -Format 'yyyyMMdd-HHmmss-ffff'))
    New-Item -ItemType Directory -Path $taskBackup | Out-Null
    Copy-Item -LiteralPath $taskDestination -Destination (Join-Path $taskBackup '2k Launcher Rust Prototype.exe')
    Assert-QA083Idle
    Copy-Item -LiteralPath $taskSource -Destination $taskDestination
    $taskFlush = [IO.File]::Open($taskDestination,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read)
    try { $taskFlush.Flush($true) } finally { $taskFlush.Dispose() }
    if ((Get-FileHash -LiteralPath $taskDestination).Hash -ne $taskExpected) { throw 'Dev copy verification failed.' }
    $taskViteUp = $false
    try { $taskViteUp = (Invoke-WebRequest -UseBasicParsing -TimeoutSec 2 'http://localhost:5173/').StatusCode -eq 200 } catch {}
    if (-not $taskViteUp) {
        Start-Process -FilePath 'C:\Program Files\nodejs\node.exe' -ArgumentList 'node_modules/vite/bin/vite.js','--host','localhost','--port','5173','--strictPort' -WorkingDirectory $taskRoot -WindowStyle Hidden | Out-Null
    }
    Assert-QA083Idle
    # Dev-only read-only timing observer; the normal provider child inherits this.
    $taskPreviousShadow = [Environment]::GetEnvironmentVariable('C27_OUTFIT_READY_SHADOW','Process')
    try {
        $env:C27_OUTFIT_READY_SHADOW = '1'
    Start-Process -FilePath $taskDestination -WorkingDirectory (Split-Path $taskDestination -Parent) | Out-Null
    } finally { [Environment]::SetEnvironmentVariable('C27_OUTFIT_READY_SHADOW',$taskPreviousShadow,'Process') }
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show($_.Exception.Message, 'QA083 dev launcher') | Out-Null
    exit 1
}
