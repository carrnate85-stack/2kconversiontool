$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$writer=Join-Path (Split-Path $project -Parent) 'nba2k27-mod-launcher/runtime-resources/session-loader/write_saved_outfit_plan.ps1'
$fixture=Join-Path $project ('artifacts/outfit-plan-oracle-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture|Out-Null
$utf8=[Text.UTF8Encoding]::new($false)
$started=[datetime]::Parse('2026-10-16T08:30:00Z').ToUniversalTime()
$cases=[Collections.Generic.List[object]]::new()
function New-Player([int]$Slot,[string[]]$Items) {
 return @{choiceId=('{0:x48}' -f ($Slot+1));roster=([long]0x400000000+$Slot*0x4F8);slot=$Slot;name=('Player '+$Slot);items=@($Items);status='preview-verified-not-applied-to-match'}
}
function Add-PlanCase([string]$Name,[object[]]$Players) {
 $choices=@{schema='blacktop-match-choices-v1';pid=311;started=$started.Ticks.ToString();players=@($Players)}
 $sourcePath=Join-Path $fixture ($Name+'.json');$output=Join-Path $fixture ($Name+'.plan')
 [IO.File]::WriteAllText($sourcePath,($choices|ConvertTo-Json -Depth 12 -Compress),$utf8)
 $accepted=$true;$errorText=$null
 try {& $writer -ChoicesPath $sourcePath -GamePid 311 -Started $started -OutputPath $output 6>$null}
 catch {$accepted=$false;$errorText=$_.Exception.Message}
 $bytes=if($accepted){[BitConverter]::ToString([IO.File]::ReadAllBytes($output)).Replace('-','')}else{$null}
 $cases.Add(@{name=$Name;players=@($Players);accepted=$accepted;bytes=$bytes;error=$errorText})
}
for($count=3;$count -le 7;$count++){
 $items=@(for($i=0;$i -lt $count;$i++){'clothing_item_'+$i})
 Add-PlanCase ('pieces-'+$count) @((New-Player 0 $items))
}
Add-PlanCase 'shirtless' @((New-Player 0 @('NO SHIRT','pants_black','acc_sock_white','shoe_0000')))
Add-PlanCase 'two-players' @((New-Player 0 @('top_white','pants_black','acc_sock_white')),(New-Player 1 @('top_red','pants_black','acc_sock_white','shoe_0000')))
Add-PlanCase 'ten-players' @(for($i=0;$i -lt 10;$i++){New-Player $i @('top_white','pants_black','acc_sock_white')})
Add-PlanCase 'empty' @()
Add-PlanCase 'two-pieces' @((New-Player 0 @('top_white','pants_black')))
Add-PlanCase 'eight-pieces' @((New-Player 0 @('a','b','c','d','e','f','g','h')))
Add-PlanCase 'bad-whitespace' @((New-Player 0 @('not a valid name','b','c')))
Add-PlanCase 'long-name' @((New-Player 0 @(('x'*256),'b','c')))
Add-PlanCase 'duplicate-player' @((New-Player 0 @('a','b','c')),(New-Player 0 @('a','b','c')))
$invalid=New-Player 0 @('a','b','c');$invalid.roster=65535
Add-PlanCase 'bad-origin' @($invalid)
$result=@{schema='nba2k27-outfit-plan-oracle-v1';game=@{pid=311;started=$started.ToFileTimeUtc().ToString();path='C:\Fixture\NBA2K27.exe'};sourceSha256=(Get-FileHash -LiteralPath $writer -Algorithm SHA256).Hash;cases=$cases.ToArray();gameAccess=$false}
if($cases.Count -ne 15 -or @($cases|Where-Object accepted).Count -ne 8){throw 'Unexpected original outfit plan acceptance'}
[IO.File]::WriteAllText((Join-Path $project 'assets/outfit-plan-regressions.json'),($result|ConvertTo-Json -Depth 15 -Compress),$utf8)
[pscustomobject]@{fixture=$fixture;cases=$cases.Count;accepted=8;sourceSha256=$result.sourceSha256;gameAccess=$false}|ConvertTo-Json -Compress
