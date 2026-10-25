[CmdletBinding()]
param([switch]$Commit)

$ErrorActionPreference = 'Stop'
$project = (Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$source = Join-Path $project 'Builds\0.53.44\Share-QA141-Production\NBA 2K27 Launcher 0.53.44.exe'
$expectedHash = '34F7BFD5273C7118EA42FE626AF4FC9754A366DC261D6AA5F40F3F57FD0753CF'
$expectedBytes = 47994368
$expectedOldHash = '1D1BBAD3843001D5A08581FBBC05FBA10E8571215D8A82966A0A8FAEA1123E60'
$mainRoot = 'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\2k Launcher'
$app = Join-Path $mainRoot 'App'
$target = Join-Path $app '2k Launcher Rust.exe'

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

function Get-TreeManifest([string]$Path) {
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
    throw 'QA141 share executable differs from the exact validated build.'
}
if ($oldHash -ne $expectedOldHash) {
    throw "Installed Main changed since inspection. Expected $expectedOldHash, found $oldHash."
}

$before = [ordered]@{
    data = Get-TreeManifest (Join-Path $mainRoot 'Data')
    library = Get-TreeManifest (Join-Path $mainRoot 'Library')
    app = Get-TreeManifest $app
}

if (-not $Commit) {
    [pscustomobject]@{
        ready = $true
        commit = $false
        source = $source
        sourceSha256 = $sourceHash
        target = $target
        installedSha256 = $oldHash
        dataFiles = $before.data.Count
        libraryFiles = $before.library.Count
    } | ConvertTo-Json -Depth 5
    exit 0
}

$stamp = Get-Date -Format 'yyyyMMdd-HHmmss-ffff'
$recovery = Join-Path $mainRoot 'Recovery'
$backup = Join-Path $recovery "App-before-QA141-$stamp"
$stage = Join-Path $recovery "App-next-QA141-$stamp"
$failed = Join-Path $recovery "App-failed-QA141-$stamp"
$workspaceReceipt = Join-Path $project "artifacts\qa141-main-promotion-$stamp.json"
$mainReceipt = Join-Path $recovery "qa141-main-promotion-$stamp.json"
foreach ($taskPath in @($app,$backup,$stage,$failed)) {
    $taskAbsolute=[IO.Path]::GetFullPath($taskPath)
    if (-not $taskAbsolute.StartsWith([IO.Path]::GetFullPath($mainRoot).TrimEnd('\')+'\',[StringComparison]::OrdinalIgnoreCase)) {throw 'Promotion path escaped the named launcher directory.'}
}
$oldMoved = $false
$newMoved = $false

try {
    foreach ($path in @($backup, $stage, $failed)) {
        if (Test-Path -LiteralPath $path) { throw "Recovery destination unexpectedly exists: $path" }
    }
    Copy-Item -LiteralPath $app -Destination $stage -Recurse
    Assert-OrdinaryDirectory $stage

    Copy-Item -LiteralPath $source -Destination (Join-Path $stage '2k Launcher Rust.exe') -Force
    Copy-Item -LiteralPath (Join-Path $project 'artifacts\qa141-production\REPORT.txt') -Destination (Join-Path $stage 'QA-REPORT.md') -Force
    Copy-Item -LiteralPath (Join-Path $project 'Builds\0.53.44\Share-QA141-Production\README.txt') -Destination (Join-Path $stage 'README.md') -Force
    Copy-Item -LiteralPath (Join-Path $project 'frontend\parity.json') -Destination (Join-Path $stage 'FEATURE-PARITY.json') -Force
    $installedUtc = [DateTime]::UtcNow.ToString('o')
    $buildInfo = [ordered]@{
        version = '0.53.44'
        candidate = 'QA141'
        builtUtc = $sourceItem.LastWriteTimeUtc.ToString('o')
        installedUtc = $installedUtc
        sha256 = $expectedHash
        bytes = $expectedBytes
        frontendTestsPassed = 252
        typeScriptPassed = $true
        frontendProductionBuildPassed = $true
        rustFormatPassed = $false
        rustReleaseCheckPassed = $false
        optimizedReleaseBuildPassed = $true
        isolatedUiLoaded = $true
        isolatedSmokeChecks = 13
        isolatedSmokePassed = $false
        isolatedSmokeQualification = 'See qa141-production/smoke-results.json for exact candidate checks; prior QA106 Season History remains unresolved.'
        nativeDownloadTestsPassed = 25
        startupVideoProbePassed = $false
        livePlayAcceptancePending = $true
        productionFeatures = @('custom-protocol')
        devFeaturesEnabled = $false
        mainInstalled = $true
    } | ConvertTo-Json -Depth 5
    [IO.File]::WriteAllText((Join-Path $stage 'BUILD-INFO.json'), $buildInfo + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))

    $stageExe = Join-Path $stage '2k Launcher Rust.exe'
    if ((Get-Item -LiteralPath $stageExe).Length -ne $expectedBytes -or (Get-FileHash -LiteralPath $stageExe -Algorithm SHA256).Hash -ne $expectedHash) {
        throw 'Staged Main executable verification failed.'
    }
    foreach ($taskName in @('2k Launcher Rust.exe','BUILD-INFO.json','QA-REPORT.md','README.md','FEATURE-PARITY.json')) {
        $taskFlush=[IO.File]::Open((Join-Path $stage $taskName),[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read)
        try {$taskFlush.Flush($true)} finally {$taskFlush.Dispose()}
    }
    $stageManifest = Get-TreeManifest $stage
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
        throw 'Installed QA141 executable verification failed.'
    }
    $backupManifest = Get-TreeManifest $backup
    if (($before.app | ConvertTo-Json -Depth 5 -Compress) -cne ($backupManifest | ConvertTo-Json -Depth 5 -Compress)) {
        throw 'Recovery App differs from the pre-promotion App.'
    }
    $after = [ordered]@{
        data = Get-TreeManifest (Join-Path $mainRoot 'Data')
        library = Get-TreeManifest (Join-Path $mainRoot 'Library')
        app = Get-TreeManifest $app
    }
    if (($before.data | ConvertTo-Json -Depth 5 -Compress) -cne ($after.data | ConvertTo-Json -Depth 5 -Compress) -or
        ($before.library | ConvertTo-Json -Depth 5 -Compress) -cne ($after.library | ConvertTo-Json -Depth 5 -Compress)) {
        throw 'Data or Library changed during promotion.'
    }
    if (($stageManifest | ConvertTo-Json -Depth 5 -Compress) -cne ($after.app | ConvertTo-Json -Depth 5 -Compress)) {
        throw 'Installed App differs from the staged App.'
    }

    $receipt = [ordered]@{
        candidate = 'QA141'
        version = '0.53.44'
        installedUtc = $installedUtc
        source = $source
        installed = $target
        backup = $backup
        previousSha256 = $oldHash
        installedSha256 = $installedHash
        bytes = $expectedBytes
        dataPreserved = $true
        libraryPreserved = $true
        appBackupVerified = $true
        gameOrLauncherStartedDuringPromotion = $false
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

