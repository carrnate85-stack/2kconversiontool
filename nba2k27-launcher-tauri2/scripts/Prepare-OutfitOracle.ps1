$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$original=Join-Path (Split-Path $project -Parent) 'nba2k27-roster-research/src/BlacktopTool.cs'
$source=[IO.File]::ReadAllText($original)
function Slice-Original([string]$Start,[string]$End) {
 $a=$source.IndexOf($Start);$b=$source.IndexOf($End,$a)
 if($a -lt 0 -or $b -le $a){throw 'Original outfit policy boundaries changed'}
 return $source.Substring($a,$b-$a)
}
$types=Slice-Original 'public sealed class OutfitLayout' 'public sealed class UniformContext'
$layout=(Slice-Original 'static bool ValidateLayout' 'static bool ValidPrivateItems').Replace('static bool ValidateLayout','public static bool ValidateLayout')
$pool=(Slice-Original 'static string ClothingPool' 'static long Q(').Replace('static string ClothingPool','public static string ClothingPool')
$page=Slice-Original 'var data=new byte[4096];' 'memory.Write(allocation,data);'
$policy='using System;using System.Linq;using System.Text;using System.Collections.Generic;using System.Text.RegularExpressions;'+$types+@'
public sealed class BlacktopPlayer {public long description,model,roster,sourcePointer;public List<OutfitOption> outfits=new List<OutfitOption>();}
public static class OriginalPolicy {
 public static FixtureMemory memory;
 const long StreetMagic=0x32464F4C43373243L;
 static long Q(long a){return BitConverter.ToInt64(memory.Read(a,8),0);}
 static string Text(long a,int bytes){return Encoding.Unicode.GetString(memory.Read(a,bytes)).Split('\0')[0];}
 public static byte[] Page(long allocation,int gamePid,long gameStarted,BlacktopPlayer p,string[] items,int streetIndex,bool rosterShoes){
'@+$page+'return data;}'+$layout+$pool+'}'
$fixture=Join-Path $project ('artifacts/outfit-policy-oracle-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture|Out-Null
$utf8=[Text.UTF8Encoding]::new($false)
[IO.File]::WriteAllText((Join-Path $fixture 'OriginalPolicy.cs'),$policy,$utf8)
$exe=Join-Path $fixture 'OutfitPolicyFixture.exe'
$csc=Join-Path $env:WINDIR 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
& $csc /nologo /target:exe /main:OutfitPolicyFixture ("/out:"+$exe) /r:System.Web.Extensions.dll (Join-Path $fixture 'OriginalPolicy.cs') (Join-Path $project 'test/OutfitPolicyFixture.cs')
if($LASTEXITCODE -ne 0){throw 'Original outfit policy fixture did not compile'}
$raw=& $exe (Join-Path (Split-Path $project -Parent) 'nba2k27-mod-launcher/src/data/blacktop-preset-layouts.json')
if($LASTEXITCODE -ne 0){throw 'Original outfit policy fixture failed'}
$data=$raw|ConvertFrom-Json
if($data.schema -ne 'nba2k27-outfit-policy-oracle-v1' -or $data.records -ne 1791 -or $data.validLayouts -ne 1791 -or $data.changedMarkersRejected -ne 1791 -or $data.pages.Count -ne 12 -or $data.gameAccess -ne $false){throw 'Unexpected outfit oracle shape'}
$data|Add-Member -NotePropertyName sourceSha256 -NotePropertyValue (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash
[IO.File]::WriteAllText((Join-Path $project 'assets/outfit-policy-regressions.json'),($data|ConvertTo-Json -Depth 9 -Compress),$utf8)
[pscustomobject]@{fixture=$fixture;groups=$data.groups;records=$data.records;layoutChecks=3582;pages=$data.pages.Count;sourceSha256=$data.sourceSha256;gameAccess=$false}|ConvertTo-Json -Compress
