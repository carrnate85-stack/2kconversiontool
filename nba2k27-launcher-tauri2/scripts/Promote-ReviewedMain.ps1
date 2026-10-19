# Fixed, engineering-only release promotion. This is not a user updater.
# No process termination, installer, arbitrary destination or recursive deletion.
# Without -Commit this performs retained read-only checks and writes NOTHING.
[CmdletBinding()]
param([switch]$Commit)
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'Promote-ReviewedMain.Core.ps1')
$taskProject=[ReviewedMainFs]::Full((Split-Path $PSScriptRoot -Parent))
$taskMain='C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\2k Launcher'
$taskBuild=Join-Path $taskProject 'Builds/0.36.0'
$taskPublisherPath=Join-Path $taskProject 'artifacts/saved-media-publish-main-race-fixed.exe'
$taskPins=[Collections.Generic.List[IDisposable]]::new()
$taskPublisher=$null
try {
    Open-PromotionAncestors (Split-Path $taskPublisherPath -Parent) $taskPins
    $taskPublisher=[ReviewedMainFs]::Open($taskPublisherPath,$false,$false)
    if((Get-PromotionHash $taskPublisher) -ne 'BEE9D8F3AA8A995314F4B937ACD4CD638F7312A649D5D34D8636DD3197433A17'){throw 'Reviewed media inspector executable differs'}
    $taskInspect={
        $taskOutput=@(& $taskPublisherPath '--inspect-reviewed')
        if($LASTEXITCODE -ne 0){throw 'Saved-media full retained-identity inspection failed'}
        return ($taskOutput -join "`n"|ConvertFrom-Json -ErrorAction Stop)
    }.GetNewClosure()
    Invoke-ReviewedPromotionCore -Root $taskMain -Build $taskBuild `
        -ExpectedOldExeSha '5D35A27176E91547FDCC6BCC09017A2AD0E0030C12C5A9FEA7F207BBDF0D227E' `
        -MediaCheck $taskInspect -Commit:$Commit | ConvertTo-Json -Depth 10
} finally { if($null -ne $taskPublisher){$taskPublisher.Dispose()};foreach($taskPin in $taskPins){$taskPin.Dispose()} }
