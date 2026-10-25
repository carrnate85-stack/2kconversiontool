[CmdletBinding()]
param([switch]$Commit)

$ErrorActionPreference = 'Stop'
$project = (Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$source = Join-Path $project 'Builds\0.53.1\Share-QA097-Debug-Menus\NBA 2K27 Launcher 0.53.1.exe'
$expectedHash = '049C1B68B24330AED0A3C3602E9C3C2F583865B82C64B32406E7E14A282AC570'
$expectedBytes = 46705664
$mainRoot = 'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\2k Launcher'
$app = Join-Path $mainRoot 'App'
$target = Join-Path $app '2k Launcher Rust.exe'
$expectedOldHash = 'B86C1AAFA291DA5850DA93A90796F15A84EA57B225393F83591761D56316F958'

function Assert-OrdinaryDirectory([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Expected an ordinary directory: $Path"
    }
}

function Assert-OrdinaryFile([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.LinkType) {
        throw "Expected an ordinary file: $Path"
    }
}

function Assert-LauncherClosed {
    $busy = @(Get-Process | Where-Object {
        $_.ProcessName -match '^(NBA2K27|EasyAntiCheat.*|EACLaunch|start_protected_game|2k Launcher.*|2k-launcher-.*|nba2k27-launcher-tauri2)$'
    })
    if ($busy.Count) {
        throw ('Close the game and launcher normally first: ' + (($busy | ForEach-Object { "$($_.ProcessName) (PID $($_.Id))" }) -join ', '))
    }
}

function Get-TreeSummary([string]$Path) {
    $files = @(Get-ChildItem -LiteralPath $Path -File -Recurse -Force | Sort-Object FullName)
    [ordered]@{
        files = $files.Count
        bytes = [int64](($files | Measure-Object Length -Sum).Sum)
        names = @($files | ForEach-Object { $_.FullName.Substring($Path.Length).TrimStart('\') })
    }
}

function Get-AppManifest([string]$Path) {
    @(
        Get-ChildItem -LiteralPath $Path -File -Recurse -Force |
            Sort-Object FullName |
            ForEach-Object {
                [ordered]@{
                    path = $_.FullName.Substring($Path.Length).TrimStart('\')
                    bytes = $_.Length
                    sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
                }
            }
    )
}

Assert-OrdinaryDirectory $mainRoot
Assert-OrdinaryDirectory $app
Assert-OrdinaryDirectory (Join-Path $mainRoot 'Data')
Assert-OrdinaryDirectory (Join-Path $mainRoot 'Library')
Assert-OrdinaryDirectory (Join-Path $mainRoot 'Recovery')
Assert-OrdinaryFile $source
Assert-OrdinaryFile $target
Assert-LauncherClosed

$sourceItem = Get-Item -LiteralPath $source
$sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
$oldHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
if ($sourceItem.Length -ne $expectedBytes -or $sourceHash -ne $expectedHash) {
    throw 'QA097 share executable differs from the exact reviewed build.'
}
if ($oldHash -ne $expectedOldHash) {
    throw "Installed Main changed since inspection. Expected $expectedOldHash, found $oldHash."
}

$before = [ordered]@{
    data = Get-TreeSummary (Join-Path $mainRoot 'Data')
    library = Get-TreeSummary (Join-Path $mainRoot 'Library')
    app = Get-AppManifest $app
}

if (-not $Commit) {
    [pscustomobject]@{
        ready = $true
        commit = $false
        source = $source
        sourceSha256 = $sourceHash
        target = $target
        installedSha256 = $oldHash
        dataFiles = $before.data.files
        libraryFiles = $before.library.files
    } | ConvertTo-Json -Depth 5
    exit 0
}

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss-ffff'
$recovery = Join-Path $mainRoot 'Recovery'
$backup = Join-Path $recovery "App-before-QA097-$stamp"
$stage = Join-Path $recovery "App-next-QA097-$stamp"
$failed = Join-Path $recovery "App-failed-QA097-$stamp"
$workspaceReceipt = Join-Path $project "artifacts\qa097-main-promotion-$stamp.json"
$mainReceipt = Join-Path $recovery "qa097-main-promotion-$stamp.json"
$oldMoved = $false
$newMoved = $false

try {
    if ((Test-Path -LiteralPath $backup) -or (Test-Path -LiteralPath $stage) -or (Test-Path -LiteralPath $failed)) {
        throw 'Promotion recovery destination unexpectedly exists.'
    }
    Copy-Item -LiteralPath $app -Destination $stage -Recurse
    Assert-OrdinaryDirectory $stage

    Copy-Item -LiteralPath $source -Destination (Join-Path $stage '2k Launcher Rust.exe') -Force
    Copy-Item -LiteralPath (Join-Path $project 'artifacts\qa097-release\REPORT.md') -Destination (Join-Path $stage 'QA-REPORT.md') -Force
    Copy-Item -LiteralPath (Join-Path $project 'Builds\0.53.1\Share-QA097-Debug-Menus\README.txt') -Destination (Join-Path $stage 'README.md') -Force
    Copy-Item -LiteralPath (Join-Path $project 'frontend\parity.json') -Destination (Join-Path $stage 'FEATURE-PARITY.json') -Force
    $buildInfo = [ordered]@{
        version = '0.53.1'
        candidate = 'QA097'
        builtUtc = $sourceItem.LastWriteTimeUtc.ToString('o')
        installedUtc = [DateTime]::UtcNow.ToString('o')
        sha256 = $expectedHash
        bytes = $expectedBytes
        offlineValidationPassed = $true
        liveDebugMenuAcceptancePending = $true
        mainInstalled = $true
    } | ConvertTo-Json -Depth 5
    [IO.File]::WriteAllText((Join-Path $stage 'BUILD-INFO.json'), $buildInfo + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))

    $stageExe = Join-Path $stage '2k Launcher Rust.exe'
    if ((Get-Item -LiteralPath $stageExe).Length -ne $expectedBytes -or (Get-FileHash -LiteralPath $stageExe -Algorithm SHA256).Hash -ne $expectedHash) {
        throw 'Staged Main executable verification failed.'
    }
    $stageManifest = Get-AppManifest $stage
    Assert-LauncherClosed
    if ((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne $expectedOldHash) {
        throw 'Installed Main changed before promotion.'
    }

    Move-Item -LiteralPath $app -Destination $backup
    $oldMoved = $true
    Move-Item -LiteralPath $stage -Destination $app
    $newMoved = $true

    $installedHash = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
    if ($installedHash -ne $expectedHash -or (Get-Item -LiteralPath $target).Length -ne $expectedBytes) {
        throw 'Installed QA097 executable verification failed.'
    }
    $backupManifest = Get-AppManifest $backup
    if (($before.app | ConvertTo-Json -Depth 5 -Compress) -cne ($backupManifest | ConvertTo-Json -Depth 5 -Compress)) {
        throw 'Recovery App differs from the pre-promotion App.'
    }
    $after = [ordered]@{
        data = Get-TreeSummary (Join-Path $mainRoot 'Data')
        library = Get-TreeSummary (Join-Path $mainRoot 'Library')
        app = Get-AppManifest $app
    }
    if (($before.data | ConvertTo-Json -Depth 5 -Compress) -cne ($after.data | ConvertTo-Json -Depth 5 -Compress) -or
        ($before.library | ConvertTo-Json -Depth 5 -Compress) -cne ($after.library | ConvertTo-Json -Depth 5 -Compress)) {
        throw 'Data or Library inventory changed during promotion.'
    }
    if (($stageManifest | ConvertTo-Json -Depth 5 -Compress) -cne ($after.app | ConvertTo-Json -Depth 5 -Compress)) {
        throw 'Installed App differs from the staged App.'
    }

    $receipt = [ordered]@{
        candidate = 'QA097'
        version = '0.53.1'
        installedUtc = [DateTime]::UtcNow.ToString('o')
        source = $source
        installed = $target
        backup = $backup
        previousSha256 = $oldHash
        installedSha256 = $installedHash
        bytes = $expectedBytes
        dataPreserved = $true
        libraryPreserved = $true
        appBackupVerified = $true
        gameOrLauncherStarted = $false
    }
    $json = $receipt | ConvertTo-Json -Depth 6
    foreach ($path in @($workspaceReceipt, $mainReceipt)) {
        [IO.File]::WriteAllText($path, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
        $stream = [IO.File]::Open($path, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::Read)
        try { $stream.Flush($true) } finally { $stream.Dispose() }
    }
    $receipt | ConvertTo-Json -Depth 6
} catch {
    $failure = $_
    try {
        if ($newMoved -and (Test-Path -LiteralPath $app)) {
            Move-Item -LiteralPath $app -Destination $failed
            $newMoved = $false
        }
        if ($oldMoved -and -not (Test-Path -LiteralPath $app) -and (Test-Path -LiteralPath $backup)) {
            Move-Item -LiteralPath $backup -Destination $app
            $oldMoved = $false
        }
    } catch {
        throw "Promotion failed and rollback also failed. Promotion: $($failure.Exception.Message) Rollback: $($_.Exception.Message)"
    }
    throw $failure
}
