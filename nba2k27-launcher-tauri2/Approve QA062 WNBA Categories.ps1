param(
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedDevExeSha256,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedGameDllSha256,
    [switch]$CheckOnly
)
$ErrorActionPreference='Stop'
$taskRoot=$PSScriptRoot
$taskPortable=Join-Path $taskRoot 'Dev\Portable'
$taskExecutable=Join-Path $taskPortable 'App\2k Launcher Rust Prototype.exe'
$taskStage=Join-Path $taskRoot 'Dev\Pending\QA062\launcher.exe'
$taskPack=Join-Path $taskRoot 'artifacts\qa058-wnba-private-wad-wpf-era-20261018'
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
Assert-Hash $taskExecutable $ExpectedDevExeSha256
Assert-Hash $taskStage $ExpectedDevExeSha256
$taskLeases=@(Get-ChildItem -LiteralPath (Join-Path $taskPortable 'Data') -Filter 'blacktop-outfit-v2-*.lease' -File)
if($taskLeases.Count -ne 1) {throw 'Expected exactly one current dev Play lease. Start QA062 dev with Play first.'}
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
    [IO.Path]::GetFileName($taskGamePath) -ine 'NBA2K27.exe') {throw 'The current game is not owned by this exact QA062 dev launcher.'}
Assert-Process $taskGamePid $taskGameBirth $taskGamePath
Assert-Process $taskWorkerPid $taskWorkerBirth $taskWorkerPath
Assert-Process $taskParentPid $taskParentBirth $taskParentPath
Assert-Hash $taskGamePath $taskGameHash
Assert-Hash (Join-Path ([IO.Path]::GetDirectoryName($taskGamePath)) 'version.dll') $ExpectedGameDllSha256
Assert-Hash (Join-Path $taskPack '0A') '6630BEDCBF708145FDFB55A48DE467265F3DE29CD76081E9C24A1C8EFD7B84BD'
Assert-Hash (Join-Path $taskPack '0B') 'D42B4E4DDFA03A83DF799A49A9BAEF1526A74393F4CFC44A5CE01AA2BF7CE7EE'
$taskSourceReport=Join-Path $taskPortable ('Logs\game-session\wnba-source-test-'+$taskGamePid+'.log')
Assert-OrdinaryPath $taskSourceReport
$taskSourceText=[IO.File]::ReadAllText($taskSourceReport)
if($taskSourceText.Length -gt 4096 -or
   $taskSourceText -notmatch '(?m)^STATE=REGISTERED\r?$' -or
   $taskSourceText -notmatch ('(?m)^PID='+$taskGamePid+'\r?$') -or
   $taskSourceText -notmatch ('(?m)^SESSION='+$taskNonce+'\r?$')) {
    throw 'Run the QA062 WNBA source test at Home first and require REGISTERED for this exact game session.'
}
Assert-Hash (Join-Path $taskPortable 'Data\wnba-priority-v2\qa058-wpf-era\0A') '6630BEDCBF708145FDFB55A48DE467265F3DE29CD76081E9C24A1C8EFD7B84BD'
Assert-Hash (Join-Path $taskPortable 'Data\wnba-priority-v2\qa058-wpf-era\0B') 'D42B4E4DDFA03A83DF799A49A9BAEF1526A74393F4CFC44A5CE01AA2BF7CE7EE'
if($CheckOnly) {Write-Output 'Exact QA062 dev/game, private pack and source registration verified. No category approval written.';exit 0}
Add-Type -AssemblyName System.Windows.Forms
$taskAnswer=[Windows.Forms.MessageBox]::Show(
    'This DEV test enables WNBA category navigation in Freestyle after the registered mixed roster passes the existing deep checks. It changes only the four verified category-order values for this game session. The old roster-loader swap stays disabled. Player and gameplay compatibility still need testing. After approval enter the Freestyle picker and leave it open until the result appears. Close the game normally after testing. Continue?',
    'WNBA Freestyle category test',[Windows.Forms.MessageBoxButtons]::YesNo,[Windows.Forms.MessageBoxIcon]::Information)
if($taskAnswer -ne [Windows.Forms.DialogResult]::Yes) {Write-Output 'Cancelled. No category approval was written.';exit 0}
Assert-Process $taskGamePid $taskGameBirth $taskGamePath
Assert-Process $taskWorkerPid $taskWorkerBirth $taskWorkerPath
Assert-Process $taskParentPid $taskParentBirth $taskParentPath
if([Convert]::ToBase64String([IO.File]::ReadAllBytes($taskLeases[0].FullName)) -ne [Convert]::ToBase64String($taskLease)) {throw 'Play lease changed while preparing the test.'}
$taskDestination=Join-Path $taskPortable 'Data\wnba-priority-v2'
Assert-OrdinaryPath $taskDestination
$taskApproval=Join-Path $taskDestination ('categories-'+$taskNonce+'.bin')
if(Test-Path -LiteralPath $taskApproval) {throw 'This game session already has a category approval; it will not be replaced.'}
$taskToken=New-Object byte[] 72
[Array]::Copy([Text.Encoding]::ASCII.GetBytes('C27WNPC1'),0,$taskToken,0,8)
[Array]::Copy([BitConverter]::GetBytes([uint32]$taskGamePid),0,$taskToken,8,4)
[Array]::Copy([BitConverter]::GetBytes([uint64]$taskGameBirth),0,$taskToken,16,8)
[Array]::Copy([Text.Encoding]::ASCII.GetBytes($taskNonce),0,$taskToken,24,48)
$taskTemporary=Join-Path $taskDestination ('.categories-'+$taskNonce+'-'+[Guid]::NewGuid().ToString('N')+'.tmp')
$taskStream=[IO.File]::Open($taskTemporary,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
try {$taskStream.Write($taskToken,0,$taskToken.Length);$taskStream.Flush($true)} finally {$taskStream.Dispose()}
Move-Item -LiteralPath $taskTemporary -Destination $taskApproval -ErrorAction Stop
$taskStatus=Join-Path $taskPortable ('Logs\native-sessions\'+$taskNonce+'\provider-state.json')
$taskProgress=New-Object Windows.Forms.Form
$taskProgress.Text='WNBA categories - enter the Freestyle picker'
$taskProgress.Width=650;$taskProgress.Height=210;$taskProgress.StartPosition='CenterScreen';$taskProgress.ControlBox=$false
$taskLabel=New-Object Windows.Forms.Label
$taskLabel.Dock=[Windows.Forms.DockStyle]::Fill;$taskLabel.Padding=New-Object Windows.Forms.Padding(18)
$taskLabel.Text='Category test approved. Enter the NBA Learn2K / Freestyle team picker and leave it open. You can return focus to the game.'
$taskProgress.Controls.Add($taskLabel);$taskProgress.Show()
$taskDeadline=[DateTime]::UtcNow.AddSeconds(120)
$taskResult='PENDING: No active category result yet. Leave the picker open and report this state.'
try {
    while([DateTime]::UtcNow -lt $taskDeadline) {
        try {
            $taskState=Get-Content -LiteralPath $taskStatus -Raw | ConvertFrom-Json
            if($taskState.session -ceq $taskNonce) {
                $taskFeature=$taskState.v2Features.features.'wnba-freestyle'
                if($null -ne $taskFeature) {
                    $taskLabel.Text='Enter the Freestyle picker and leave it open.'+[Environment]::NewLine+[Environment]::NewLine+$taskFeature.phase+': '+$taskFeature.message
                    if($taskFeature.phase -in @('active','held','unavailable','owner-closed')) {
                        $taskResult=$taskFeature.phase.ToUpperInvariant()+': '+$taskFeature.message;break
                    }
                }
            }
        } catch [IO.IOException] {} catch [System.ArgumentException] {}
        [Windows.Forms.Application]::DoEvents();Start-Sleep -Milliseconds 100
    }
} finally {$taskProgress.Close();$taskProgress.Dispose()}
if($taskResult.StartsWith('ACTIVE:')) {$taskResult += ' Try cycling team categories to WNBA. Report whether the team names and player list appear before entering a court.'}
[Windows.Forms.MessageBox]::Show($taskResult,'WNBA Freestyle category result',[Windows.Forms.MessageBoxButtons]::OK,[Windows.Forms.MessageBoxIcon]::Information) | Out-Null
