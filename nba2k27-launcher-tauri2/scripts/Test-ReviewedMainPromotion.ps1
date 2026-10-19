# Synthetic workspace fixtures only. Never calls the fixed Main wrapper, real
# publisher, Get-CimInstance, game, Steam or an application executable.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot 'Promote-ReviewedMain.Core.ps1')
$taskProject=[ReviewedMainFs]::Full((Split-Path $PSScriptRoot -Parent))
$taskFixtures=Join-Path $taskProject ('artifacts/promotion-fixtures-'+[guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($taskFixtures)|Out-Null
$script:taskChecks=0
function Assert-Fixture([bool]$Condition,[string]$Message){if(-not $Condition){throw $Message};$script:taskChecks++}
function Write-Fixture([string]$Path,[string]$Text){[IO.Directory]::CreateDirectory((Split-Path $Path -Parent))|Out-Null;[IO.File]::WriteAllText($Path,$Text,[Text.UTF8Encoding]::new($false))}
function Write-FixtureJson([string]$Path,$Value){Write-Fixture $Path ($Value|ConvertTo-Json -Depth 15)}
function New-Fixture([string]$Name){
    $base=Join-Path $taskFixtures $Name;$main=Join-Path $base 'Main';$build=Join-Path $base 'Build'
    foreach($dir in @('App','Recovery','Data/Rust','Library/Original','Runtime/keep')){[IO.Directory]::CreateDirectory((Join-Path $main $dir))|Out-Null}
    Write-Fixture (Join-Path $main 'App/2k Launcher WPF.exe') 'old WPF fixture'
    Write-Fixture (Join-Path $main 'App/backend/nested/readme.txt') 'old backend bytes retained'
    [IO.Directory]::CreateDirectory((Join-Path $main 'App/empty'))|Out-Null
    Write-Fixture (Join-Path $main 'Data/settings.json') 'old WPF settings retained byte-for-byte'
    Write-Fixture (Join-Path $main 'Data/Rust/settings.json') 'existing Rust settings also retained'
    Write-Fixture (Join-Path $main 'Library/Original/music.bin') 'original music'
    Write-Fixture (Join-Path $main 'Runtime/keep/runtime.bin') 'old runtime'
    Write-Fixture (Join-Path $main 'Recovery/rust-saved-media-publication.jsonl') 'fixture publisher receipt (trusted injected inspector)'
    Write-Fixture (Join-Path $build 'Portable/App/2k Launcher Rust Prototype.exe') 'new Rust executable fixture'
    foreach($name in @('FEATURE-PARITY.json','QA-REPORT.md','README.md','THIRD-PARTY-NOTICES.txt')){Write-Fixture (Join-Path $build ('Portable/'+$name)) ('new release '+$name)}
    $exe=Get-Item -LiteralPath (Join-Path $build 'Portable/App/2k Launcher Rust Prototype.exe')
    $sha=(Get-FileHash -LiteralPath $exe.FullName -Algorithm SHA256).Hash
    $info=@{version='0.36.0';sha256=$sha;bytes=$exe.Length;isolatedUiVerified=$true;uiChecks=105}
    Write-FixtureJson (Join-Path $build 'Portable/BUILD-INFO.json') $info
    $qa=@{version='0.36.0';exeSha256=$sha;exeBytes=$exe.Length;pipelineBuildVerified=$true;pipelineUiChecks=105;http=@{pass=$true};httpFixtureExited=$true;providerWorkerFixtureExited=$true;providerWorker=@{passed=$true};testOnlyMarkersAbsentFromRelease=$true;dataAndLibraryPreserved=$true;automatedGameAccess=$false;gameVersion2LegacyMemoryWriteGateEnabled=$false;repeats=@(@{exited=$true;wpfValidatorsPassed=$true;checks=105},@{exited=$true;wpfValidatorsPassed=$true;checks=105},@{exited=$true;wpfValidatorsPassed=$true;checks=105})}
    Write-FixtureJson (Join-Path $build 'QA-VERIFICATION.json') $qa
    $mp4=@{version='0.36.0';exeSha256=$sha;pass=$true;decodedVideos=4;decodedFramePositions=12;audioOutput=$false;gameAccess=$false;mainChanged=$false;rows=@(@{passed=$true;points=@(0,1,2);width=1280;height=720},@{passed=$true;points=@(0,1,2);width=1280;height=720},@{passed=$true;points=@(0,1,2);width=1280;height=720},@{passed=$true;points=@(0,1,2);width=1280;height=720})}
    Write-FixtureJson (Join-Path $build 'SAVED-MP4-VERIFICATION.json') $mp4
    return [pscustomobject]@{root=$main;build=$build;oldHash=(Get-FileHash -LiteralPath (Join-Path $main 'App/2k Launcher WPF.exe')).Hash}
}
function Invoke-Fixture($Fixture,[switch]$Commit,[scriptblock]$Fault={param($Point)},[scriptblock]$Closed={}){
    $media={ [pscustomobject]@{phase='complete-verified';units=5;reusedVideos=4;stageInventorySha256='76b3d222e19e7c6f577452a3aee3f3e8964ab52fb5199b0b69e1778832ca8ef0';originalsUnchanged=$true;settingsPreserved=$true;gameAccess=$false;appChanged=$false} }
    Invoke-ReviewedPromotionCore -Root $Fixture.root -Build $Fixture.build -ExpectedOldExeSha $Fixture.oldHash -MediaCheck $media -ClosedCheck $Closed -Commit:$Commit -Fault $Fault
}
function Expect-FixtureFailure([scriptblock]$Body,[string]$Message){$failed=$false;try{& $Body|Out-Null}catch{$failed=$true};Assert-Fixture $failed $Message}
function Get-FixtureInventory([string]$Root){$tree=Open-PromotionTree $Root;try{return $tree.digest}finally{Close-PromotionTree $tree}}

$dry=New-Fixture 'dry-run'
$before=Get-FixtureInventory $dry.root
$report=Invoke-Fixture $dry
Assert-Fixture ($report.phase -ceq 'dry-run-verified' -and -not $report.writes) 'Dry run did not report no writes'
Assert-Fixture ((Get-FixtureInventory $dry.root) -ceq $before) 'Dry run changed fixture bytes, IDs or membership'

$ok=New-Fixture 'success'
$saved=@{}
foreach($rel in @('Data/settings.json','Data/Rust/settings.json','Library/Original/music.bin','Runtime/keep/runtime.bin')){$saved[$rel]=(Get-FileHash -LiteralPath (Join-Path $ok.root $rel)).Hash}
$oldTree=Get-FixtureInventory (Join-Path $ok.root 'App')
$report=Invoke-Fixture $ok -Commit
$backup=Join-Path $ok.root 'Recovery/App-before-Rust-0.36.0-20261016'
Assert-Fixture ($report.phase -ceq 'complete') 'Promotion did not complete'
Assert-Fixture ((Get-FixtureInventory $backup) -ceq $oldTree) 'Old App was not moved intact, including empty directories and IDs'
Assert-Fixture (@(Get-ChildItem -LiteralPath (Join-Path $ok.root 'App') -File).Count -eq 6) 'New App allowlist differs'
Assert-Fixture (-not (Test-Path -LiteralPath (Join-Path $ok.root 'App/2k Launcher WPF.exe'))) 'Old WPF remains in active App'
foreach($rel in $saved.Keys){Assert-Fixture ((Get-FileHash -LiteralPath (Join-Path $ok.root $rel)).Hash -eq $saved[$rel]) "Preserved path changed: $rel"}
$events=@(Get-Content -LiteralPath $report.receipt|ForEach-Object {$_|ConvertFrom-Json})
Assert-Fixture ($events[-1].event -ceq 'complete') 'Complete receipt is missing'
Assert-Fixture ($events[0].data.oldManifest.Count -eq 5) 'Receipt did not preserve complete old inventory'
Expect-FixtureFailure {Invoke-Fixture $ok -Commit} 'Repeat promotion did not hold existing backup/receipt'

foreach($point in @('after-copy:2k Launcher Rust.exe','staged','before-old-rename','after-old-rename','before-new-rename')){
    $f=New-Fixture ('fault-'+($point -replace '[^A-Za-z0-9-]','_'));$old=Get-FixtureInventory (Join-Path $f.root 'App')
    $wantedPoint=$point
    $hook={param($Point) if($Point -ceq $wantedPoint){throw ('synthetic '+$Point)}}.GetNewClosure()
    Expect-FixtureFailure {Invoke-Fixture $f -Commit -Fault $hook} ('Fault did not fail: '+$point)
    Assert-Fixture ((Get-FixtureInventory (Join-Path $f.root 'App')) -ceq $old) ('Old App not restored/preserved: '+$point)
    Assert-Fixture (@(Get-ChildItem -LiteralPath (Join-Path $f.root 'Recovery') -Directory -Filter 'App-next-*').Count -eq 1) 'Partial stage was unexpectedly removed'
    if($point -in @('after-old-rename','before-new-rename')){
        Assert-Fixture (-not (Test-Path -LiteralPath (Join-Path $f.root 'Recovery/App-before-Rust-0.36.0-20261016'))) 'Rollback duplicated rather than moved the old App'
        $last=Get-Content -LiteralPath (Join-Path $f.root 'Recovery/rust-main-app-promotion-0.36.0-20261016.jsonl')|Select-Object -Last 1|ConvertFrom-Json
        Assert-Fixture ($last.event -ceq 'rolled-back') 'Durable rollback acknowledgement is missing'
    }
}
$lateGame=New-Fixture 'process-started-after-old-move'
$state=[pscustomobject]@{blocked=$false}
$lateClosed={if($state.blocked){throw 'synthetic new protected process'}}.GetNewClosure()
$lateHook={param($Point)if($Point -ceq 'after-old-rename'){$state.blocked=$true}}.GetNewClosure()
Expect-FixtureFailure {Invoke-Fixture $lateGame -Commit -Closed $lateClosed -Fault $lateHook} 'A newly running game was not held'
Assert-Fixture (-not (Test-Path -LiteralPath (Join-Path $lateGame.root 'App'))) 'Rollback was attempted despite a newly running protected process'
Assert-Fixture (Test-Path -LiteralPath (Join-Path $lateGame.root 'Recovery/App-before-Rust-0.36.0-20261016/2k Launcher WPF.exe')) 'Old backup was not retained after late process guard'
$post=New-Fixture 'after-new-rename'
Expect-FixtureFailure {Invoke-Fixture $post -Commit -Fault {param($Point) if($Point -ceq 'after-new-rename'){throw 'synthetic acknowledgement fault'}}} 'Post-commit fault did not hold'
Assert-Fixture (Test-Path -LiteralPath (Join-Path $post.root 'App/2k Launcher Rust.exe')) 'Published new App was removed after ambiguous completion'
Assert-Fixture (Test-Path -LiteralPath (Join-Path $post.root 'Recovery/App-before-Rust-0.36.0-20261016/2k Launcher WPF.exe')) 'Old backup was removed after ambiguous completion'

$foreign=New-Fixture 'foreign-app-race'
$foreignRoot=$foreign.root
$hook={param($Point) if($Point -ceq 'before-new-rename'){[IO.Directory]::CreateDirectory([IO.Path]::Combine($foreignRoot,'App'))|Out-Null;[IO.File]::WriteAllText([IO.Path]::Combine($foreignRoot,'App/foreign.txt'),'foreign bytes')}}.GetNewClosure()
Expect-FixtureFailure {Invoke-Fixture $foreign -Commit -Fault $hook} 'Foreign App creation was not held'
Assert-Fixture ([IO.File]::ReadAllText((Join-Path $foreign.root 'App/foreign.txt')) -ceq 'foreign bytes') 'Foreign App was replaced by rollback'
Assert-Fixture (Test-Path -LiteralPath (Join-Path $foreign.root 'Recovery/App-before-Rust-0.36.0-20261016/2k Launcher WPF.exe')) 'Owned backup was lost when rollback destination was foreign'

foreach($collision in @('Recovery/App-before-Rust-0.36.0-20261016','Recovery/rust-main-app-promotion-0.36.0-20261016.jsonl')){
    $f=New-Fixture ('collision-'+[guid]::NewGuid().ToString('N'));Write-Fixture (Join-Path $f.root $collision) 'foreign existing'
    $before=Get-FixtureInventory $f.root
    Expect-FixtureFailure {Invoke-Fixture $f -Commit} 'Existing destination was not held'
    Assert-Fixture ((Get-FixtureInventory $f.root) -ceq $before) 'Collision changed an existing tree'
}
foreach($broken in @('exe','qa','mp4','unknown-app-file','old-exe','linked-app','closed')){
    $f=New-Fixture ('invalid-'+$broken)
    switch($broken){
        'exe' {Write-Fixture (Join-Path $f.build 'Portable/App/2k Launcher Rust Prototype.exe') 'modified'}
        'qa' {$v=Get-Content -LiteralPath (Join-Path $f.build 'QA-VERIFICATION.json') -Raw|ConvertFrom-Json;$v.exeSha256='0'*64;Write-FixtureJson (Join-Path $f.build 'QA-VERIFICATION.json') $v}
        'mp4' {$v=Get-Content -LiteralPath (Join-Path $f.build 'SAVED-MP4-VERIFICATION.json') -Raw|ConvertFrom-Json;$v.pass=$false;Write-FixtureJson (Join-Path $f.build 'SAVED-MP4-VERIFICATION.json') $v}
        'unknown-app-file' {Write-Fixture (Join-Path $f.build 'Portable/App/foreign.dll') 'unreviewed'}
        'old-exe' {$f.oldHash='0'*64}
        'linked-app' {New-Item -ItemType Junction -Path (Join-Path $f.root 'App/linked') -Target (Join-Path $f.root 'Library')|Out-Null}
    }
    $oldHash=(Get-FileHash -LiteralPath (Join-Path $f.root 'App/2k Launcher WPF.exe')).Hash
    if($broken -eq 'closed'){Expect-FixtureFailure {Invoke-Fixture $f -Commit -Closed {throw 'synthetic running game'}} 'Running process gate did not hold'}
    else{Expect-FixtureFailure {Invoke-Fixture $f -Commit} ('Invalid evidence was not refused: '+$broken)}
    Assert-Fixture ((Get-FileHash -LiteralPath (Join-Path $f.root 'App/2k Launcher WPF.exe')).Hash -eq $oldHash) 'Evidence refusal changed the old executable'
    Assert-Fixture (-not (Test-Path -LiteralPath (Join-Path $f.root 'Recovery/rust-main-app-promotion-0.36.0-20261016.jsonl'))) 'Evidence refusal wrote a receipt'
}

foreach($name in @('NBA2K27.exe','EasyAntiCheat_EOS.exe','start_protected_game.exe','2k Launcher WPF.exe','2k Launcher Rust.exe','2k Launcher Rust Prototype.exe','2k-Launcher-0.36.45.exe')){
    Expect-FixtureFailure {Assert-PromotionProcesses @([pscustomobject]@{Name=$name;ExecutablePath='C:\known\process.exe';CommandLine='';ProcessId=42})} ('Process not blocked: '+$name)
}
foreach($runtime in @('node.exe','electron.exe')){
    Expect-FixtureFailure {Assert-PromotionProcesses @([pscustomobject]@{Name=$runtime;ExecutablePath=$null;CommandLine='';ProcessId=43})} 'Unknown runtime was not held'
    Expect-FixtureFailure {Assert-PromotionProcesses @([pscustomobject]@{Name=$runtime;ExecutablePath='C:\old\App\backend\node.exe';CommandLine='';ProcessId=44})} 'Known old backend was not held'
}
Assert-PromotionProcesses @([pscustomobject]@{Name='node.exe';ExecutablePath='C:\tools\node.exe';CommandLine='C:\tools\unrelated.js';ProcessId=45})
Assert-Fixture $true 'Known unrelated runtime should be allowed'
Expect-FixtureFailure {Get-PromotionPath $taskFixtures '../escape'} 'Relative path escape was not rejected'
Expect-FixtureFailure {[ReviewedMainFs]::Full('C:\test\bad:stream')} 'Alternate stream was not rejected'

$result=[ordered]@{pass=$true;checks=$script:taskChecks;fixtures=$taskFixtures;mainAccess=$false;processAccess=$false;gameAccess=$false;productionWrapperInvoked=$false;fixtureFilesRetained=$true}
$result|ConvertTo-Json -Depth 5
