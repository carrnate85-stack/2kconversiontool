param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
# Use the host's own built-in modules even when launched from another PowerShell environment.
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Management\Microsoft.PowerShell.Management.psd1')
Import-Module (Join-Path $PSHOME 'Modules\Microsoft.PowerShell.Utility\Microsoft.PowerShell.Utility.psd1')
$taskRoot = $PSScriptRoot
$taskSource = Join-Path $taskRoot 'Dev\Pending\QA138\launcher.exe'
$taskExpected = 'F56E6AB39C8E6581A387B82EA323D03157AC10A9A05B864D581812060B051F42'
$taskExpectedBytes = 48973312
$taskLog = Join-Path $taskRoot 'artifacts\qa138-camera-button-placement\starter.log'

function Assert-QA138Idle {
    $taskBusy = @(Get-Process | Where-Object {
        $_.ProcessName -match '^(NBA2K27|2k Launcher.*|2k-launcher-.*|nba2k27-launcher-tauri2|EasyAntiCheat.*)$'
    })
    if ($taskBusy.Count) {
        throw ('Close the game and launcher normally first. Nothing was replaced. Still running: ' +
            (($taskBusy | ForEach-Object { "$($_.ProcessName) (PID $($_.Id))" }) -join ', '))
    }
}

try {
    Add-Content -LiteralPath $taskLog -Value ((Get-Date -Format o) + ' Starting QA138 dev check')
    if ((Get-Item -LiteralPath $taskSource).Length -ne $taskExpectedBytes) {
        throw 'QA138 staged size changed; refusing activation.'
    }
    if ((Get-FileHash -LiteralPath $taskSource -Algorithm SHA256).Hash -ne $taskExpected) {
        throw 'QA138 staged dev binary changed; refusing activation.'
    }
    Assert-QA138Idle
    if ($CheckOnly) {
        Write-Output 'QA138 staged hash verified; game/launcher closed; no activation performed.'
        exit 0
    }

    $taskDestination = Join-Path $taskRoot 'Dev\Portable\App\2k Launcher Rust Prototype.exe'
    Add-Content -LiteralPath $taskLog -Value 'Staged hash verified. Backing up the prior dev executable.'
    $taskBackup = Join-Path $taskRoot ('Backups\before-qa138-dev-activation-' + (Get-Date -Format 'yyyyMMdd-HHmmss-ffff'))
    New-Item -ItemType Directory -Path $taskBackup | Out-Null
    Copy-Item -LiteralPath $taskDestination -Destination (Join-Path $taskBackup '2k Launcher Rust Prototype.exe')
    Assert-QA138Idle
    Copy-Item -LiteralPath $taskSource -Destination $taskDestination
    $taskFlush = [IO.File]::Open($taskDestination,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read)
    try { $taskFlush.Flush($true) } finally { $taskFlush.Dispose() }
    if ((Get-FileHash -LiteralPath $taskDestination -Algorithm SHA256).Hash -ne $taskExpected) {
        throw 'QA138 dev copy verification failed.'
    }
    Assert-QA138Idle
    $taskPreviousShadow = [Environment]::GetEnvironmentVariable('C27_OUTFIT_READY_SHADOW','Process')
    try {
        $env:C27_OUTFIT_READY_SHADOW = '1'
        Start-Process -FilePath $taskDestination -WorkingDirectory (Split-Path $taskDestination -Parent) | Out-Null
        Add-Content -LiteralPath $taskLog -Value 'Dev process started.'
    } finally {
        [Environment]::SetEnvironmentVariable('C27_OUTFIT_READY_SHADOW',$taskPreviousShadow,'Process')
    }
} catch {
    Add-Content -LiteralPath $taskLog -Value $_.Exception.ToString()
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show($_.Exception.Message, 'QA138 dev launcher') | Out-Null
    exit 1
}
