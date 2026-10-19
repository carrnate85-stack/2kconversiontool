$ErrorActionPreference='Stop'
$taskProject=Split-Path $PSScriptRoot -Parent
$taskSource=Join-Path (Split-Path $taskProject -Parent) 'nba2k27-mod-launcher/unlocks-helper'
$taskFiles=@((Join-Path $taskSource 'CategoryPolicy.cs'),(Join-Path $taskSource 'FreestylePolicy.cs'))
$taskHashes=@($taskFiles|ForEach-Object {(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash})
$taskFixture=Join-Path $taskProject ('artifacts/freestyle-category-oracle-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskFixture|Out-Null
$taskRunner=Join-Path $taskFixture 'FreestyleCategoryFixture.exe'
$taskCsc=Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $taskCsc /nologo /target:exe /platform:x64 ("/out:"+$taskRunner) /r:System.dll /r:System.Core.dll /r:System.Web.Extensions.dll $taskFiles (Join-Path $taskProject 'test/FreestyleCategoryFixture.cs') *> (Join-Path $taskFixture 'compile.log')
if($LASTEXITCODE -ne 0){throw 'Pure original Freestyle policy fixture compilation failed'}
$taskRaw=& $taskRunner
if($LASTEXITCODE -ne 0){throw 'Pure original Freestyle policy fixture failed'}
$taskData=$taskRaw|ConvertFrom-Json
if($taskData.schema -ne 'nba2k27-freestyle-category-oracle-v1' -or $taskData.gameAccess -ne $false -or $taskData.mixtures.Count -ne 16 -or $taskData.foreign.Count -ne 36){throw 'Unexpected Freestyle oracle contract'}
$taskAfter=@($taskFiles|ForEach-Object {(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash})
if(($taskHashes -join ',') -ne ($taskAfter -join ',')){throw 'Original Freestyle policy sources changed during fixture compilation'}
$taskData|Add-Member -NotePropertyName sourceSha256 -NotePropertyValue $taskHashes
[IO.File]::WriteAllText((Join-Path $taskProject 'assets/freestyle-category-regressions.json'),($taskData|ConvertTo-Json -Depth 9),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{fixture=$taskFixture;mixtures=16;foreign=36;sourceSha256=$taskHashes;gameAccess=$false;originalHelperChanged=$false}|ConvertTo-Json -Compress
