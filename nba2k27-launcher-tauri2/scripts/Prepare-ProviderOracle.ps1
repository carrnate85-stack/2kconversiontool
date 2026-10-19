param([ValidateSet('Wire','Assets')][string]$Kind='Wire')
$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$original=Join-Path (Split-Path $project -Parent) 'nba2k27-mod-launcher/provider-host/Program.cs'
$source=[IO.File]::ReadAllText($original)
$start=$source.IndexOf('static class Limits')
$end=$source.IndexOf('static class NativeMethods',$start)
if($start -lt 0 -or $end -le $start){throw 'Original provider protocol boundaries changed'}
$fixture=Join-Path $project ('artifacts/provider-'+$Kind.ToLowerInvariant()+'-oracle-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture|Out-Null
$utf8=[Text.UTF8Encoding]::new($false)
if($Kind -eq 'Wire'){
 $protocol="using System.Buffers.Binary;`nusing System.Text;`n"+$source.Substring($start,$end-$start)
 $fixtureClass='ProviderProtocolFixture'
}else{
 $classStart=$source.IndexOf('sealed class ProviderService')
 $usingEnd=$source.IndexOf('if (args is')
 if($classStart -lt 0 -or $usingEnd -lt 1){throw 'Original provider service boundaries changed'}
 $protocol=$source.Substring(0,$usingEnd)+$source.Substring($classStart)
 $fixtureClass='ProviderAssetsFixture'
}
[IO.File]::WriteAllText((Join-Path $fixture 'OriginalProtocol.cs'),$protocol,$utf8)
Copy-Item -LiteralPath (Join-Path $project ('test/'+$fixtureClass+'.cs')) -Destination (Join-Path $fixture 'Fixture.cs')
$csproj='<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>enable</ImplicitUsings><Nullable>enable</Nullable><EnableNETAnalyzers>false</EnableNETAnalyzers><NuGetAudit>false</NuGetAudit><UseSharedCompilation>false</UseSharedCompilation></PropertyGroup></Project>'
[IO.File]::WriteAllText((Join-Path $fixture 'Fixture.csproj'),$csproj,$utf8)
[IO.File]::WriteAllText((Join-Path $fixture 'NuGet.Config'),'<configuration><packageSources><clear /></packageSources></configuration>',$utf8)
$dotnet=Join-Path $env:ProgramFiles 'dotnet/dotnet.exe'
$env:DOTNET_CLI_HOME=Join-Path $fixture 'dotnet-home'
$env:DOTNET_NOLOGO='1'
$env:DOTNET_CLI_TELEMETRY_OPTOUT='1'
$env:DOTNET_SKIP_FIRST_TIME_EXPERIENCE='1'
& $dotnet build (Join-Path $fixture 'Fixture.csproj') --configfile (Join-Path $fixture 'NuGet.Config') -o (Join-Path $fixture 'out') -v quiet
if($LASTEXITCODE -ne 0){throw 'Original provider protocol fixture compilation failed'}
$raw=& $dotnet (Join-Path $fixture 'out/Fixture.dll') (Join-Path $fixture 'profile')
if($LASTEXITCODE -ne 0){throw 'Original provider protocol fixture failed'}
$data=$raw|ConvertFrom-Json
if($Kind -eq 'Wire'){
 if($data.schema -ne 'nba2k27-provider-wire-oracle-v1' -or $data.requests.Count -ne 650 -or $data.responses.Count -ne 18 -or $data.errors.Count -ne 7 -or $data.gameAccess -ne $false){throw "Unexpected provider oracle shape: $($data.requests.Count) requests"}
 $asset='assets/provider-wire-regressions.json'
}else{
 if($data.schema -ne 'nba2k27-provider-assets-oracle-v1' -or $data.steps.Count -ne 71 -or $data.files.Count -ne 11 -or $data.gameAccess -ne $false){throw "Unexpected provider asset oracle shape: $($data.steps.Count) steps"}
 $asset='assets/provider-assets-regressions.json'
 if($data.folds.Count -lt 1000 -or $data.folds.Count -gt 4096){throw 'Unexpected provider ordinal equivalence table'}
 $ordinal=[ordered]@{schema='nba2k27-provider-ordinal-v1';algorithm='StringComparer.OrdinalIgnoreCase scalar equivalence';runtime=$data.runtime;sourceSha256=(Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash;folds=$data.folds}
 [IO.File]::WriteAllText((Join-Path $project 'assets/provider-ordinal.json'),($ordinal|ConvertTo-Json -Depth 5 -Compress),$utf8)
}
[IO.File]::WriteAllText((Join-Path $project $asset),($raw -join "`n"),$utf8)
[pscustomobject]@{kind=$Kind;requests=$data.requests.Count;steps=$data.steps.Count;responses=$data.responses.Count;errors=$data.errors.Count;sourceSha256=(Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash;fixture=$fixture;gameAccess=$false;networkRequests=0}|ConvertTo-Json -Compress
