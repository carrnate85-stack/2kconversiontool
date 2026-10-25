[CmdletBinding()]
param([switch]$Commit)

$ErrorActionPreference = 'Stop'
$project = (Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$sourceRoot = Join-Path $project 'Dev\Portable\Data\wnba-priority-v2'
$mainRoot = 'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\2k Launcher'
$destinationRoot = Join-Path $mainRoot 'Data\wnba-priority-v2'
$files = @(
    [pscustomobject]@{ Relative='qa058-wpf-era\0A'; Bytes=120; Hash='6630BEDCBF708145FDFB55A48DE467265F3DE29CD76081E9C24A1C8EFD7B84BD' },
    [pscustomobject]@{ Relative='qa058-wpf-era\0B'; Bytes=1762144; Hash='D42B4E4DDFA03A83DF799A49A9BAEF1526A74393F4CFC44A5CE01AA2BF7CE7EE' },
    [pscustomobject]@{ Relative='qa064-blacktop-source\0A'; Bytes=144; Hash='0C25D0839B11B4AF085F587C1BAF4E74F821CD34D2B902FC950DBC91E5123CC9' },
    [pscustomobject]@{ Relative='qa064-blacktop-source\0B'; Bytes=1762144; Hash='D42B4E4DDFA03A83DF799A49A9BAEF1526A74393F4CFC44A5CE01AA2BF7CE7EE' }
)

function Assert-PlainDirectory([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Expected a plain directory: $Path"
    }
}

function Assert-PlainFile([string]$Path, [int64]$Bytes, [string]$Hash) {
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or $item.LinkType) {
        throw "Expected a plain file: $Path"
    }
    if ($item.Length -ne $Bytes -or (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ne $Hash) {
        throw "WNBA pack file differs from the verified bytes: $Path"
    }
}

$busy = @(Get-Process | Where-Object {
    $_.ProcessName -match '^(NBA2K27|EasyAntiCheat.*|EACLaunch|start_protected_game|2k Launcher.*|2k-launcher-.*|nba2k27-launcher-tauri2)$'
})
if ($busy.Count) { throw 'The game or launcher is running.' }
Assert-PlainDirectory $sourceRoot
Assert-PlainDirectory (Join-Path $mainRoot 'Data')
foreach ($file in $files) {
    Assert-PlainFile (Join-Path $sourceRoot $file.Relative) $file.Bytes $file.Hash
}

if (-not $Commit) {
    [pscustomobject]@{ ready=$true; source=$sourceRoot; destination=$destinationRoot; files=$files.Count } | ConvertTo-Json
    exit 0
}

if (Test-Path -LiteralPath $destinationRoot) {
    throw "Refusing to replace an existing WNBA pack directory: $destinationRoot"
}
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss-ffff'
$stage = Join-Path (Join-Path $mainRoot 'Data') "wnba-priority-v2-next-$stamp"
$receipt = Join-Path $project "artifacts\qa102-wnba-pack-$stamp.json"
try {
    New-Item -ItemType Directory -Path $stage | Out-Null
    foreach ($file in $files) {
        $target = Join-Path $stage $file.Relative
        New-Item -ItemType Directory -Path (Split-Path $target -Parent) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $sourceRoot $file.Relative) -Destination $target
        Assert-PlainFile $target $file.Bytes $file.Hash
        $stream = [IO.File]::Open($target,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read)
        try { $stream.Flush($true) } finally { $stream.Dispose() }
    }
    Move-Item -LiteralPath $stage -Destination $destinationRoot
    Assert-PlainDirectory $destinationRoot
    foreach ($file in $files) {
        Assert-PlainFile (Join-Path $destinationRoot $file.Relative) $file.Bytes $file.Hash
    }
    $body = [ordered]@{
        candidate='QA102'
        installedUtc=[DateTime]::UtcNow.ToString('o')
        source=$sourceRoot
        destination=$destinationRoot
        files=$files
        priorDestinationAbsent=$true
        existingDataFilesModified=0
        gameOrLauncherStarted=$false
    }
    $json = $body | ConvertTo-Json -Depth 6
    [IO.File]::WriteAllText($receipt,$json+[Environment]::NewLine,[Text.UTF8Encoding]::new($false))
    $stream = [IO.File]::Open($receipt,[IO.FileMode]::Open,[IO.FileAccess]::ReadWrite,[IO.FileShare]::Read)
    try { $stream.Flush($true) } finally { $stream.Dispose() }
    $body | ConvertTo-Json -Depth 6
} catch {
    if (Test-Path -LiteralPath $stage) {
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
    throw
}

