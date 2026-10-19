$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$original=Join-Path (Split-Path $project -Parent) 'nba2k27-mod-launcher/arena-helper/ArenaTool.cs'
$fixture=Join-Path $project ('artifacts/arena-plan-oracle-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture|Out-Null
$csc=Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
$exe=Join-Path $fixture 'ArenaPlanFixture.exe'
& $csc /nologo /target:exe /main:ArenaPlanFixture ("/out:"+$exe) /r:System.Web.Extensions.dll $original (Join-Path $project 'test/ArenaPlanFixture.cs')
if($LASTEXITCODE -ne 0){throw 'Original arena pure-plan fixture compilation failed'}
$raw=& $exe
if($LASTEXITCODE -ne 0){throw 'Original arena pure-plan fixture failed'}
$data=$raw|ConvertFrom-Json
if($data.schema -ne 'nba2k27-arena-plan-oracle-v1' -or $data.gameAccess -ne $false -or $data.cases.Count -ne 13 -or $data.builds.Count -ne 2){throw 'Unexpected arena fixture shape'}
$data|Add-Member -NotePropertyName sourceSha256 -NotePropertyValue (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash
[IO.File]::WriteAllText((Join-Path $project 'assets/arena-plan-regressions.json'),($data|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{fixture=$fixture;cases=$data.cases.Count;builds=$data.builds.Count;sourceSha256=$data.sourceSha256;gameAccess=$false}|ConvertTo-Json -Compress
