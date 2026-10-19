param(
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedDevExeSha256,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedGameDllSha256,
    [long]$ExpectedDevExeBytes=0, # PENDING FINAL QA065 EXE SIZE
    [long]$ExpectedGameDllBytes=0, # PENDING FINAL QA065 DLL SIZE
    [switch]$CheckOnly
)
$ErrorActionPreference='Stop'
$taskRoot=$PSScriptRoot
$taskPortable=Join-Path $taskRoot 'Dev\Portable'
$taskExecutable=Join-Path $taskPortable 'App\2k Launcher Rust Prototype.exe'
$taskStage=Join-Path $taskRoot 'Dev\Pending\QA065\launcher.exe'
$taskPack=Join-Path $taskRoot 'artifacts\qa064-wnba-blacktop-source-20261018'
$taskGameHash='29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1'
function Assert-OrdinaryPath([string]$Path) {
    $taskItem=Get-Item -LiteralPath $Path -Force
    while($null -ne $taskItem) {
        if(($taskItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw 'Reparse paths are not accepted for this dev diagnostic.' }
        if($taskItem -is [IO.FileInfo]) {$taskItem=$taskItem.Directory} else {$taskItem=$taskItem.Parent}
    }
}
function Assert-Process([uint32]$ProcessId,[uint64]$Birth,[string]$Path) {
    $taskProcess=Get-Process -Id $ProcessId -ErrorAction Stop
    if([uint64]$taskProcess.StartTime.ToUniversalTime().ToFileTimeUtc() -ne $Birth -or
       $taskProcess.Path -ine $Path) {throw 'A process identity changed; no diagnostic approval was written.'}
}
function Assert-Hash([string]$Path,[string]$Hash) {
    Assert-OrdinaryPath $Path
    if((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash -ine $Hash) {throw ('Pinned file changed: '+$Path)}
}
if($ExpectedDevExeSha256 -eq ('0'*64) -or $ExpectedGameDllSha256 -eq ('0'*64)) {throw 'QA065 final pins are pending; category approval is disabled.'}
if($ExpectedDevExeBytes -le 0 -or $ExpectedGameDllBytes -le 0) {throw 'QA065 final binary sizes are pending; approval is disabled.'}
if((Get-Item -LiteralPath $taskExecutable).Length -ne $ExpectedDevExeBytes -or (Get-Item -LiteralPath $taskStage).Length -ne $ExpectedDevExeBytes) {throw 'QA065 dev executable size changed.'}
Assert-Hash $taskExecutable $ExpectedDevExeSha256
Assert-Hash $taskStage $ExpectedDevExeSha256
$taskLeases=@(Get-ChildItem -LiteralPath (Join-Path $taskPortable 'Data') -Filter 'blacktop-outfit-v2-*.lease' -File)
if($taskLeases.Count -ne 1) {throw 'Expected exactly one current dev Play lease. Start QA065 dev with Play first.'}
Assert-OrdinaryPath $taskLeases[0].FullName
$taskLease=[IO.File]::ReadAllBytes($taskLeases[0].FullName)
if($taskLease.Length -ne 1632 -or [Text.Encoding]::ASCII.GetString($taskLease,0,8) -ne '2KOV2L01') {throw 'Unexpected Play lease format.'}
$taskNonce=$taskLeases[0].BaseName.Substring('blacktop-outfit-v2-'.Length)
if($taskNonce -notmatch '^[0-9a-fA-F]{48}$') {throw 'Invalid dev Play session nonce.'}
if([Text.Encoding]::ASCII.GetString($taskLease,48,48) -cne $taskNonce) {throw 'Play lease filename and body nonce differ.'}
$taskGamePid=[BitConverter]::ToUInt32($taskLease,8)
$taskWorkerPid=[BitConverter]::ToUInt32($taskLease,12)
$taskParentPid=[BitConverter]::ToUInt32($taskLease,16)
$taskGameBirth=[BitConverter]::ToUInt64($taskLease,24)
$taskWorkerBirth=[BitConverter]::ToUInt64($taskLease,32)
$taskParentBirth=[BitConverter]::ToUInt64($taskLease,40)
$taskGamePath=[Text.Encoding]::Unicode.GetString($taskLease,96,512).Split([char]0)[0]
$taskWorkerPath=[Text.Encoding]::Unicode.GetString($taskLease,608,512).Split([char]0)[0]
$taskParentPath=[Text.Encoding]::Unicode.GetString($taskLease,1120,512).Split([char]0)[0]
if($taskWorkerPath -ine $taskExecutable -or $taskParentPath -ine $taskExecutable -or
    [IO.Path]::GetFileName($taskGamePath) -ine 'NBA2K27.exe') {throw 'The current game is not owned by this exact QA065 dev launcher.'}
Assert-Process $taskGamePid $taskGameBirth $taskGamePath
Assert-Process $taskWorkerPid $taskWorkerBirth $taskWorkerPath
Assert-Process $taskParentPid $taskParentBirth $taskParentPath
Assert-Hash $taskGamePath $taskGameHash
$taskDllPath=Join-Path ([IO.Path]::GetDirectoryName($taskGamePath)) 'version.dll'
if((Get-Item -LiteralPath $taskDllPath).Length -ne $ExpectedGameDllBytes) {throw 'QA065 game DLL size changed.'}
Assert-Hash $taskDllPath $ExpectedGameDllSha256
Assert-Hash (Join-Path $taskPack '0A') '0C25D0839B11B4AF085F587C1BAF4E74F821CD34D2B902FC950DBC91E5123CC9'
Assert-Hash (Join-Path $taskPack '0B') 'D42B4E4DDFA03A83DF799A49A9BAEF1526A74393F4CFC44A5CE01AA2BF7CE7EE'
$taskSourceReport=Join-Path $taskPortable ('Logs\game-session\wnba-source-test-'+$taskGamePid+'.log')
Assert-OrdinaryPath $taskSourceReport
if((Get-Item -LiteralPath $taskSourceReport).Length -gt 4096) {throw 'Source receipt exceeds its read bound.'}
$taskSourceText=[IO.File]::ReadAllText($taskSourceReport)
if($taskSourceText.Length -gt 4096 -or
   $taskSourceText -notmatch '(?m)^STATE=REGISTERED\r?$' -or
   $taskSourceText -notmatch ('(?m)^PID='+$taskGamePid+'\r?$') -or
   $taskSourceText -notmatch ('(?m)^SESSION='+$taskNonce+'\r?$')) {
    throw 'Run the QA065 WNBA source test at Home first and require REGISTERED for this exact game session.'
}
Assert-Hash (Join-Path $taskPortable 'Data\wnba-priority-v2\qa064-blacktop-source\0A') '0C25D0839B11B4AF085F587C1BAF4E74F821CD34D2B902FC950DBC91E5123CC9'
Assert-Hash (Join-Path $taskPortable 'Data\wnba-priority-v2\qa064-blacktop-source\0B') 'D42B4E4DDFA03A83DF799A49A9BAEF1526A74393F4CFC44A5CE01AA2BF7CE7EE'

$taskDestination=Join-Path $taskPortable 'Data\wnba-priority-v2'
$taskApproval=Join-Path $taskDestination ('blacktop-categories-'+$taskNonce+'.bin')
$taskSourceApproval=Join-Path $taskDestination ('approval-'+$taskNonce+'.bin')
function Assert-CategoryPrerequisites {
    $taskSettingsFile=Join-Path $taskPortable 'Data\Rust\settings.json'
    Assert-OrdinaryPath $taskSettingsFile
    if((Get-Item -LiteralPath $taskSettingsFile).Length -gt 65536) {throw 'Settings exceed their read bound.'}
    $taskSettings=Get-Content -LiteralPath $taskSettingsFile -Raw | ConvertFrom-Json
    if($taskSettings.unlockWnbaFreestyle -isnot [bool] -or $taskSettings.unlockWnbaFreestyle -ne $false) {throw 'Freestyle unlock must remain OFF for this separate Blacktop trial.'}
    if(Test-Path -LiteralPath (Join-Path $taskDestination ('categories-'+$taskNonce+'.bin'))) {throw 'A Freestyle category approval exists; use a fresh source-only session.'}
    if(Test-Path -LiteralPath $taskApproval) {throw 'This exact Blacktop category approval already exists and will not be replaced.'}
    Assert-OrdinaryPath $taskSourceApproval
    if((Get-Item -LiteralPath $taskSourceApproval).Length -ne 72) {throw 'Source approval length is invalid.'}
    $taskSourceToken=[IO.File]::ReadAllBytes($taskSourceApproval)
    if([Text.Encoding]::ASCII.GetString($taskSourceToken,0,8) -cne 'C27WNBT1' -or
       [BitConverter]::ToUInt32($taskSourceToken,8) -ne $taskGamePid -or
       [BitConverter]::ToUInt32($taskSourceToken,12) -ne 0 -or
       [BitConverter]::ToUInt64($taskSourceToken,16) -ne $taskGameBirth -or
       [Text.Encoding]::ASCII.GetString($taskSourceToken,24,48) -cne $taskNonce) {throw 'Exact two-entry source-trial approval does not match this game.'}
    Assert-OrdinaryPath $taskSourceReport
    if((Get-Item -LiteralPath $taskSourceReport).Length -gt 4096) {throw 'Source receipt exceeds its read bound.'}
    $taskReceipt=[IO.File]::ReadAllText($taskSourceReport)
    foreach($taskRequired in @('(?m)^SCHEMA=wnba-native-source-admission-test-v1\r?$','(?m)^STATE=REGISTERED\r?$','(?m)^CATEGORY_WRITES=0\r?$','(?m)^REASON=Blacktop source trial',('(?m)^PID='+$taskGamePid+'\r?$'),('(?m)^SESSION='+$taskNonce+'\r?$'))) {
        if($taskReceipt -notmatch $taskRequired) {throw 'The exact-session two-entry Blacktop source REGISTERED receipt is required.'}
    }
    Assert-Hash (Join-Path $taskPortable 'Data\wnba-priority-v2\qa064-blacktop-source\0A') '0C25D0839B11B4AF085F587C1BAF4E74F821CD34D2B902FC950DBC91E5123CC9'
    Assert-Hash (Join-Path $taskPortable 'Data\wnba-priority-v2\qa064-blacktop-source\0B') 'D42B4E4DDFA03A83DF799A49A9BAEF1526A74393F4CFC44A5CE01AA2BF7CE7EE'
}
Assert-CategoryPrerequisites
if($CheckOnly) {Write-Output 'Exact QA065 game/launcher/source receipt, source token and pack verified; no approval or writes performed.';exit 0}
Add-Type -AssemblyName System.Windows.Forms
$taskAnswer=[Windows.Forms.MessageBox]::Show(
    'This DEV Blacktop category trial uses the registered two-entry mixed roster. After a stable Blacktop player-selection screen passes deep roster and source checks, it changes the EuroLeague label and its data dispatch entry to a private RX bridge. The bridge is retained until game exit; the owned route is conditionally restored on leaving selection. This is a gameplay trial: player compatibility remains unverified. Keep Freestyle unlock OFF. Enter Blacktop selection after approval and leave it open. Close the game normally after testing. Continue?',
    'Approve QA065 Blacktop category trial',[Windows.Forms.MessageBoxButtons]::YesNo,[Windows.Forms.MessageBoxIcon]::Warning)
if($taskAnswer -ne [Windows.Forms.DialogResult]::Yes) {Write-Output 'Cancelled; no category approval written.';exit 0}
Assert-Process $taskGamePid $taskGameBirth $taskGamePath
Assert-Process $taskWorkerPid $taskWorkerBirth $taskWorkerPath
Assert-Process $taskParentPid $taskParentBirth $taskParentPath
if([Convert]::ToBase64String([IO.File]::ReadAllBytes($taskLeases[0].FullName)) -cne [Convert]::ToBase64String($taskLease)) {throw 'Exact Play lease changed.'}
Assert-CategoryPrerequisites
$taskToken=New-Object byte[] 72
[Array]::Copy([Text.Encoding]::ASCII.GetBytes('C27WNBC1'),0,$taskToken,0,8)
[Array]::Copy([BitConverter]::GetBytes([uint32]$taskGamePid),0,$taskToken,8,4)
[Array]::Copy([BitConverter]::GetBytes([uint64]$taskGameBirth),0,$taskToken,16,8)
[Array]::Copy([Text.Encoding]::ASCII.GetBytes($taskNonce),0,$taskToken,24,48)
$taskTemporary=Join-Path $taskDestination ('.blacktop-categories-'+$taskNonce+'-'+[Guid]::NewGuid().ToString('N')+'.tmp')
$taskStream=[IO.File]::Open($taskTemporary,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
try {$taskStream.Write($taskToken,0,$taskToken.Length);$taskStream.Flush($true)} finally {$taskStream.Dispose()}
# File.Move never replaces an existing approval.
[IO.File]::Move($taskTemporary,$taskApproval)
$taskTrace=Join-Path $taskPortable ('Logs\game-session\wnba-blacktop-categories-'+$taskGamePid+'.progress')
$taskProgress=New-Object Windows.Forms.Form
$taskProgress.Text='QA065 Blacktop categories - enter player selection'
$taskProgress.Width=700;$taskProgress.Height=240;$taskProgress.StartPosition='CenterScreen';$taskProgress.ControlBox=$false
$taskLabel=New-Object Windows.Forms.Label
$taskLabel.Dock=[Windows.Forms.DockStyle]::Fill;$taskLabel.Padding=New-Object Windows.Forms.Padding(18)
$taskLabel.Text='Approved. Enter Blacktop player selection and leave it open. You may return focus to the game. Await the exact-session progress result.'
$taskProgress.Controls.Add($taskLabel);$taskProgress.Show()
$taskDeadline=[DateTime]::UtcNow.AddSeconds(150)
$taskResult='PENDING: No verified category result yet. Leave selection open and report the pending state. Approval alone does not prove publication.'
try {
    while([DateTime]::UtcNow -lt $taskDeadline) {
        Assert-Process $taskGamePid $taskGameBirth $taskGamePath
        if(Test-Path -LiteralPath $taskTrace) {
            try {
                Assert-OrdinaryPath $taskTrace
                if((Get-Item -LiteralPath $taskTrace).Length -gt 65536) {throw 'Category progress exceeds its read bound.'}
                $taskTraceText=[IO.File]::ReadAllText($taskTrace)
                $taskLines=@($taskTraceText -split '\r?\n' | Where-Object {$_ -match ('(?:^| )SESSION='+$taskNonce+'(?: |$)')})
                if($taskLines.Count) {
                    $taskLatest=$taskLines[-1]
                    $taskLabel.Text='Blacktop category trial progress:'+[Environment]::NewLine+$taskLatest
                    if($taskLatest -match '(?:^| )STATE=(ACTIVE|PUBLISHED|HELD|FAILED|REFUSED)(?: |$)') {$taskResult=$taskLatest;break}
                }
            } catch [IO.IOException] {}
        }
        [Windows.Forms.Application]::DoEvents();Start-Sleep -Milliseconds 100
    }
} finally {$taskProgress.Close();$taskProgress.Dispose()}
[Windows.Forms.MessageBox]::Show($taskResult+[Environment]::NewLine+'Report whether WNBA team and player names appear before entering a court. Match compatibility is still unverified.','QA065 Blacktop category result',[Windows.Forms.MessageBoxButtons]::OK,[Windows.Forms.MessageBoxIcon]::Information) | Out-Null
