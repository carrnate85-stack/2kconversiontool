$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$original=Join-Path (Split-Path $project -Parent) 'nba2k27-mod-launcher/unlocks-helper/BallAssets.cs'
$source=[IO.File]::ReadAllText($original)
$start=$source.IndexOf('internal static class BallAssetPolicy {')
$end=$source.IndexOf('internal static partial class FreestyleTeams {',$start)
if($start -lt 0 -or $end -le $start){throw 'Original pure ball policy boundaries changed'}
$selector=Join-Path (Split-Path $original -Parent) 'BallSelector.cs'
$selectorSource=[IO.File]::ReadAllText($selector)
$constants=@('RecordStride','RecordAssetOffset')|ForEach-Object {
 $match=[regex]::Match($selectorSource,('internal const int '+$_+'\s*=\s*[^;]+;'))
 if(-not $match.Success){throw 'Original ball layout constant missing'}
 $match.Value
}
$fixture=Join-Path $project ('artifacts/ball-policy-oracle-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture|Out-Null
$pure='using System;using System.Linq;using System.Text;using System.Text.RegularExpressions;'+[Environment]::NewLine+$source.Substring($start,$end-$start)+[Environment]::NewLine+'internal static class BallSelectorPolicy {'+[string]::Join([Environment]::NewLine,$constants)+'}'
$policy=Join-Path $fixture 'OriginalBallPolicy.cs'
[IO.File]::WriteAllText($policy,$pure,[Text.UTF8Encoding]::new($false))
$exe=Join-Path $fixture 'BallPolicyFixture.exe'
$csc=Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $csc /nologo /target:exe /main:BallPolicyFixture ("/out:"+$exe) /r:System.Web.Extensions.dll $policy (Join-Path $project 'test/BallPolicyFixture.cs')
if($LASTEXITCODE -ne 0){throw 'Original pure ball policy compilation failed'}
$raw=& $exe
if($LASTEXITCODE -ne 0){throw 'Original pure ball policy fixture failed'}
$data=$raw|ConvertFrom-Json
if($data.schema -ne 'nba2k27-ball-policy-oracle-v1' -or $data.gameAccess -ne $false -or $data.restore.Count -ne 512 -or $data.menus.Count -ne 720 -or $data.replacements.Count -ne 20){throw 'Unexpected pure ball fixture shape'}
$data|Add-Member -NotePropertyName sourceSha256 -NotePropertyValue (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash
[IO.File]::WriteAllText((Join-Path $project 'assets/ball-policy-regressions.json'),($data|ConvertTo-Json -Depth 8),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{fixture=$fixture;restoreCases=$data.restore.Count;menuCases=$data.menus.Count;replacementCases=$data.replacements.Count;sourceSha256=$data.sourceSha256;gameAccess=$false}|ConvertTo-Json -Compress
