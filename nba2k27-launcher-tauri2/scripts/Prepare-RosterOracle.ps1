param([ValidateSet('Read','Write','Team','Uniform')][string]$Kind='Read')
$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$research=Join-Path (Split-Path $project -Parent) 'nba2k27-roster-research/src'
$fixture=Join-Path $project ('artifacts/'+$(if($Kind -eq 'Read'){'roster-oracle-'}elseif($Kind -eq 'Write'){'player-write-oracle-'}elseif($Kind -eq 'Team'){'team-write-oracle-'}else{'uniform-write-oracle-'})+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture | Out-Null
$compiler=Join-Path ([Environment]::GetFolderPath('Windows')) 'Microsoft.NET/Framework64/v4.0.30319/csc.exe'
$class=if($Kind -eq 'Read'){'RosterReadFixture'}elseif($Kind -eq 'Write'){'PlayerWriteFixture'}elseif($Kind -eq 'Team'){'TeamWriteFixture'}else{'UniformWriteFixture'}
$output=Join-Path $fixture ($class+'.exe')
$arguments=@('/nologo','/target:exe','/platform:x64','/optimize+','/codepage:65001',('/main:NBA2K27RosterResearch.'+$class),"/out:$output",'/reference:System.dll','/reference:System.Core.dll','/reference:System.Web.Extensions.dll')
foreach($file in @('Program.cs','RatingSchema.cs','SeasonStats.cs','SeasonStructure.cs','SeasonImportModel.cs','SeasonImportSnapshot.cs','SeasonImportCommit.cs','PlayerOrigins.cs','PlayerBio.cs','TeamStaff.cs','TeamStaffPatch.cs','TeamArenaPair.cs','TeamRotations.cs')){$arguments+=(Join-Path $research $file)}
$arguments+=(Join-Path $project ('test/'+$class+'.cs'))
& $compiler @arguments
if($LASTEXITCODE -ne 0){throw 'Original reader oracle compilation failed'}
$raw=if($Kind -eq 'Team'){& $output (Join-Path $fixture 'recovery')}elseif($Kind -eq 'Uniform'){& $output (Join-Path $project 'assets/uniform-write-layout.json')}else{& $output}
if($LASTEXITCODE -ne 0){throw 'Synthetic original reader oracle failed'}
$cases=$raw|ConvertFrom-Json
if($Kind -eq 'Read'){
 if($cases.Count -ne 3){throw 'Oracle case count differs'}
 $asset='assets/roster-read-regressions.json'
 [pscustomobject]@{cases=$cases.Count;players=@($cases|ForEach-Object {$_.expected.players.Count});gameAccess=$false;fixture=$fixture} | ConvertTo-Json -Compress
}elseif($Kind -eq 'Write'){
 if($cases.basic.Count -ne 4 -or $cases.fields.Count -ne 910 -or $cases.bio.Count -ne 16){throw 'Player-write oracle case count differs'}
 $asset='assets/player-write-regressions.json'
 [pscustomobject]@{basicOwnProcessWrites=$cases.basic.Count;pureFieldCases=$cases.fields.Count;pureBioCases=$cases.bio.Count;gameAccess=$false;fixture=$fixture} | ConvertTo-Json -Compress
}elseif($Kind -eq 'Team'){
 if($cases.Count -ne 12){throw 'Team oracle case count differs'}
 $asset='assets/team-write-regressions.json'
 [pscustomobject]@{ownProcessTeamCases=$cases.Count;succeeded=@($cases|Where-Object {$_.ok}).Count;gameAccess=$false;fixture=$fixture}|ConvertTo-Json -Compress
}else{
 if($cases.Count -ne 115){throw 'Uniform oracle case count differs'}
 $asset='assets/uniform-write-regressions.json'
 [pscustomobject]@{ownProcessUniformCases=$cases.Count;succeeded=@($cases|Where-Object {$_.ok}).Count;gameAccess=$false;fixture=$fixture}|ConvertTo-Json -Compress
}
[IO.File]::WriteAllText((Join-Path $project $asset),($raw -join "`n"),[Text.UTF8Encoding]::new($false))
