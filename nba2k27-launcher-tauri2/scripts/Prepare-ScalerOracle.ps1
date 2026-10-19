$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$reference=Join-Path (Split-Path $project -Parent) 'nba2k27-mod-launcher/scaler-helper'
$policy=Join-Path $reference 'ScalerPolicy.cs'
$helper=Join-Path $reference 'PlayerScaler.cs'
$fixture=Join-Path $project ('artifacts/scaler-policy-oracle-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture|Out-Null
$beforePolicy=(Get-FileHash -LiteralPath $policy -Algorithm SHA256).Hash
$beforeHelper=(Get-FileHash -LiteralPath $helper -Algorithm SHA256).Hash
$helperSource=Get-Content -LiteralPath $helper -Raw
if(-not $helperSource.Contains('desiredScale=B(F(owned.originalScale)*factor),desiredInverse=B(1f/F(desiredScale))')){throw 'Original scaler arena seed expression changed; inspect the oracle before regenerating'}
$csc=Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
$library=Join-Path $fixture 'ScalerPolicyReference.dll'
$runner=Join-Path $fixture 'ScalerPolicyFixture.exe'
& $csc /nologo /target:library /platform:x64 /optimize+ ("/out:"+$library) /r:System.dll /r:System.Core.dll /r:System.Web.Extensions.dll $policy $helper *> (Join-Path $fixture 'reference-compile.log')
if($LASTEXITCODE -ne 0){throw 'Original scaler reference compilation failed'}
& $csc /nologo /target:exe /platform:x64 ("/out:"+$runner) /r:System.dll /r:System.Core.dll /r:System.Web.Extensions.dll (Join-Path $project 'test/ScalerPolicyFixture.cs')
if($LASTEXITCODE -ne 0){throw 'Pure scaler oracle fixture compilation failed'}
$raw=& $runner $library
if($LASTEXITCODE -ne 0){throw 'Original scaler pure-method fixture failed'}
$data=$raw|ConvertFrom-Json
if($data.schema -ne 'nba2k27-scaler-policy-oracle-v1' -or $data.gameAccess -ne $false -or $data.targets.Count -ne 6 -or $data.owned.Count -ne 324 -or $data.pairs.Count -ne 86 -or $data.seeds.Count -ne 54 -or $data.arenaOwned.Count -ne 54){throw 'Unexpected scaler oracle contract'}
if((Get-FileHash -LiteralPath $policy -Algorithm SHA256).Hash -ne $beforePolicy -or (Get-FileHash -LiteralPath $helper -Algorithm SHA256).Hash -ne $beforeHelper){throw 'Original scaler reference changed during oracle compilation'}
$data|Add-Member -NotePropertyName policySha256 -NotePropertyValue $beforePolicy
$data|Add-Member -NotePropertyName helperSha256 -NotePropertyValue $beforeHelper
[IO.File]::WriteAllText((Join-Path $project 'assets/scaler-policy-regressions.json'),($data|ConvertTo-Json -Depth 9),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{fixture=$fixture;targets=$data.targets.Count;owned=$data.owned.Count;pairs=$data.pairs.Count;seeds=$data.seeds.Count;arenaOwned=$data.arenaOwned.Count;policySha256=$beforePolicy;helperSha256=$beforeHelper;gameAccess=$false;originalHelperChanged=$false}|ConvertTo-Json -Compress
