$ErrorActionPreference='Stop'
$taskProject=Split-Path $PSScriptRoot -Parent
$taskSource=Join-Path (Split-Path $taskProject -Parent) 'nba2k27-mod-launcher/video-helper'
$taskFiles=@((Join-Path $taskSource 'ControllerSkip.cs'),(Join-Path $taskSource 'GameVolume.cs'))
$taskHashes=@($taskFiles|ForEach-Object {(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash})
$taskFixture=Join-Path $taskProject ('artifacts/startup-video-policy-oracle-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskFixture|Out-Null
$taskRunner=Join-Path $taskFixture 'StartupVideoPolicyFixture.exe'
$taskCsc=Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $taskCsc /nologo /target:exe /platform:x64 /main:StartupVideoPolicyFixture ("/out:"+$taskRunner) /r:System.dll /r:System.Core.dll /r:System.Web.Extensions.dll $taskFiles (Join-Path $taskProject 'test/StartupVideoPolicyFixture.cs') *> (Join-Path $taskFixture 'compile.log')
if($LASTEXITCODE -ne 0){throw 'Original pure startup-video policy fixture compilation failed'}
$taskRaw=& $taskRunner
if($LASTEXITCODE -ne 0){throw 'Original pure startup-video policy fixture failed'}
$taskData=$taskRaw|ConvertFrom-Json
if($taskData.schema -ne 'nba2k27-startup-video-policy-oracle-v1' -or $taskData.gameAccess -ne $false -or $taskData.audioSettingsChanged -ne $false -or $taskData.pads.Count -ne 5 -or $taskData.mute.Count -ne 4){throw 'Unexpected startup-video oracle contract'}
$taskAfter=@($taskFiles|ForEach-Object {(Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash})
if(($taskHashes -join ',') -ne ($taskAfter -join ',')){throw 'Original startup-video policy sources changed'}
$taskData|Add-Member -NotePropertyName sourceSha256 -NotePropertyValue $taskHashes
[IO.File]::WriteAllText((Join-Path $taskProject 'assets/startup-video-policy-regressions.json'),($taskData|ConvertTo-Json -Depth 9),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{fixture=$taskFixture;padSequences=5;muteSequences=4;sourceSha256=$taskHashes;gameAccess=$false;audioSettingsChanged=$false;originalHelperChanged=$false}|ConvertTo-Json -Compress
