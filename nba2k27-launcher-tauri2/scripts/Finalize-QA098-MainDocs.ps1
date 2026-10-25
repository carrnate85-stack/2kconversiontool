[CmdletBinding()]
param([switch]$Commit)
$ErrorActionPreference = 'Stop'
$project = (Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$app = 'C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\2k Launcher\App'
$exe = Join-Path $app '2k Launcher Rust.exe'
$expected = '247D54EB0D64F72C59243EAEA942FF8F0F13B3F71A27DDBC4FD17968FA528438'
$busy = @(Get-Process | Where-Object { $_.ProcessName -match '^(NBA2K27|EasyAntiCheat.*|EACLaunch|start_protected_game|2k Launcher.*|2k-launcher-.*|nba2k27-launcher-tauri2)$' })
if ($busy.Count) { throw 'Close the game and launcher normally first.' }
if ((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $expected) { throw 'Installed QA098 executable changed.' }
$pairs = @(
    @((Join-Path $project 'artifacts\qa098-jukebox-scroll\REPORT.md'), (Join-Path $app 'QA-REPORT.md')),
    @((Join-Path $project 'Builds\0.53.2\Share-QA098-Jukebox-Scroll\README.txt'), (Join-Path $app 'README.md'))
)
if (-not $Commit) {
    [pscustomobject]@{ ready = $true; commit = $false; installedSha256 = $expected; documents = @($pairs | ForEach-Object { $_[1] }) } | ConvertTo-Json -Depth 4
    exit 0
}
$docBackup = Join-Path (Split-Path $app -Parent) ('Recovery\QA098-docs-before-finalize-' + (Get-Date -Format 'yyyyMMdd-HHmmss-ffff'))
New-Item -ItemType Directory -Path $docBackup | Out-Null
foreach ($stale in @(Get-ChildItem -LiteralPath $app -File -Filter '*.qa098-new-*' -Force)) {
    if ($stale.Name -notmatch '^(QA-REPORT\.md|README\.md)\.qa098-new-[0-9a-f]{32}$' -or
        ($stale.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Unexpected QA098 temporary document: $($stale.FullName)"
    }
    Move-Item -LiteralPath $stale.FullName -Destination (Join-Path $docBackup ('stale-' + $stale.Name))
}
foreach ($pair in $pairs) {
    $source = $pair[0]
    $destination = $pair[1]
    $temporary = $destination + '.qa098-new-' + [guid]::NewGuid().ToString('N')
    Copy-Item -LiteralPath $source -Destination $temporary
    $stream = [IO.File]::Open($temporary, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::Read)
    try { $stream.Flush($true) } finally { $stream.Dispose() }
    $backup = Join-Path $docBackup ([IO.Path]::GetFileName($destination))
    [IO.File]::Replace($temporary, $destination, $backup, $true)
    if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash -ne (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash) {
        throw "Installed document verification failed: $destination"
    }
}
[pscustomobject]@{ updated = $true; installedSha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash; documentBackup = $docBackup; documents = @($pairs | ForEach-Object { $_[1] }) } | ConvertTo-Json -Depth 4
