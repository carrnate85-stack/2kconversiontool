$ErrorActionPreference='Stop'
$taskProject=Split-Path $PSScriptRoot -Parent
$taskSource=Join-Path (Split-Path $taskProject -Parent) 'nba2k27-mod-launcher/src/data/scoreboard-text-adapters.json'
$taskBefore=(Get-FileHash -LiteralPath $taskSource -Algorithm SHA256).Hash
$taskData=Get-Content -LiteralPath $taskSource -Raw|ConvertFrom-Json
$taskAllowed=@('mycourt-scrimmage','mypark-simple','proam-spectator','shut-it-down','w-scrimmage')
$taskLayouts=@($taskData.layouts|Where-Object id -in $taskAllowed|ForEach-Object {
 [ordered]@{id=$_.id;asset=$_.asset;plans=@($_.plans|ForEach-Object {
  [ordered]@{member=$_.member;sourceSha256=$_.sourceSha256;edits=@($_.edits|ForEach-Object {[ordered]@{offset=$_.offset;before=$_.before;after=$_.after}})}
 })}
})
if($taskData.schema -ne 1 -or $taskLayouts.Count -ne 5 -or $taskData.targets.Count -ne 6){throw 'Production scoreboard plan contract differs'}
$taskResult=[ordered]@{schema='nba2k27-scoreboard-layout-adapters-v1';originalSourceSha256=$taskBefore;layouts=$taskLayouts;targets=@($taskData.targets|ForEach-Object {[ordered]@{key=$_.key;text=$_.text}})}
if((Get-FileHash -LiteralPath $taskSource -Algorithm SHA256).Hash -ne $taskBefore){throw 'Original scoreboard plan changed during read'}
[IO.File]::WriteAllText((Join-Path $taskProject 'assets/scoreboard-layout-adapters.json'),($taskResult|ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{layouts=$taskLayouts.Count;targets=$taskData.targets.Count;originalChanged=$false;gameProcessAccess=$false}|ConvertTo-Json -Compress
