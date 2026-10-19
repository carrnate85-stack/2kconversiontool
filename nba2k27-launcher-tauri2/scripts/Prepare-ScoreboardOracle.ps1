$ErrorActionPreference='Stop'
$taskProject=Split-Path $PSScriptRoot -Parent
$taskOriginal=Join-Path (Split-Path $taskProject -Parent) 'nba2k27-mod-launcher'
$taskFiles=@((Join-Path $taskOriginal 'scoreboard-helper/ScoreboardCatalog.cs'),(Join-Path $taskOriginal 'scoreboard-helper/ScoreboardProfile.cs'))
$taskHashes=@($taskFiles|ForEach-Object {(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash})
$taskFixture=Join-Path $taskProject ('artifacts/scoreboard-profile-oracle-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskFixture|Out-Null
$taskRunner=Join-Path $taskFixture 'ScoreboardProfileFixture.exe'
$taskCsc=Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $taskCsc /nologo /target:exe /platform:x64 ("/out:"+$taskRunner) /r:System.dll /r:System.Core.dll /r:System.Web.Extensions.dll $taskFiles (Join-Path $taskProject 'test/ScoreboardProfileFixture.cs') *> (Join-Path $taskFixture 'compile.log')
if($LASTEXITCODE -ne 0){throw 'Pure original scoreboard fixture compilation failed'}
$taskData=(& $taskRunner)|ConvertFrom-Json
if($LASTEXITCODE -ne 0 -or $taskData.schema -ne 'nba2k27-original-scoreboard-oracle-v1' -or $taskData.gameAccess -ne $false -or $taskData.choices.Count -ne 21 -or $taskData.vectors.Count -ne 882){throw 'Unexpected scoreboard oracle contract'}
$taskAfter=@($taskFiles|ForEach-Object {(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash})
if(($taskHashes -join ',') -ne ($taskAfter -join ',')){throw 'Original scoreboard sources changed'}
$taskData|Add-Member -NotePropertyName sourceSha256 -NotePropertyValue $taskHashes
[IO.File]::WriteAllText((Join-Path $taskProject 'assets/scoreboard-original-regressions.json'),($taskData|ConvertTo-Json -Depth 12),[Text.UTF8Encoding]::new($false))
$taskOriginalCatalog=Get-Content -LiteralPath (Join-Path $taskOriginal 'src/data/scoreboards.json') -Raw|ConvertFrom-Json
$taskChoices=@($taskData.choices|ForEach-Object {
 $taskChoice=$_
 $taskMeta=$taskOriginalCatalog.choices|Where-Object id -eq $taskChoice.id
 if(@($taskMeta).Count -ne 1){throw 'Scoreboard catalog metadata differs'}
 [ordered]@{id=$taskChoice.id;label=$taskMeta.label;resource=$taskChoice.resource;file=$taskChoice.file;profile=@($taskChoice.profile);requiresLayoutAdapter=($taskMeta.requiresLayoutAdapter -eq $true);selectable=($taskChoice.id -notin @('mycareer-story','flashback-openrun'))}
})
$taskCatalog=[ordered]@{schema='nba2k27-scoreboard-native-catalog-v1';sourceSha256=$taskHashes;choices=$taskChoices}
[IO.File]::WriteAllText((Join-Path $taskProject 'assets/scoreboard-native-catalog.json'),($taskCatalog|ConvertTo-Json -Depth 9),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{fixture=$taskFixture;transitions=882;choices=21;selectable=@($taskChoices|Where-Object selectable).Count;gameAccess=$false;originalHelperChanged=$false}|ConvertTo-Json -Compress
