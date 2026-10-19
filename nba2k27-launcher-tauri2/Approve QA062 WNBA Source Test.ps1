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
if($CheckOnly) {Write-Output 'Exact QA062 dev/game identities and private pack verified. No approval or pack copy was written.';exit 0}
Add-Type -AssemblyName System.Windows.Forms
$taskAnswer=[Windows.Forms.MessageBox]::Show(
    'This dev diagnostic calls the game''s native resource registration API and may change which roster loads in this game session. Go to Home or Quick Play first. After approving, keep that menu open until the progress window explicitly says REGISTERED. You can return focus to the game while it checks. WNBA category controls remain disabled. This tests source admission only, and does not establish gameplay compatibility. The prepared roster still has 53 appended players absent from its native player hash index. Close the game normally after testing. No installed archives are changed. Continue?',
    'Approve WNBA source-admission diagnostic',[Windows.Forms.MessageBoxButtons]::YesNo,[Windows.Forms.MessageBoxIcon]::Warning)
if($taskAnswer -ne [Windows.Forms.DialogResult]::Yes) {Write-Output 'Cancelled. No diagnostic approval was written.';exit 0}
$taskDestination=Join-Path $taskPortable 'Data\wnba-priority-v2'
if(-not (Test-Path -LiteralPath $taskDestination)) {New-Item -ItemType Directory -Path $taskDestination | Out-Null}
Assert-OrdinaryPath $taskDestination
$taskPrivatePack=Join-Path $taskDestination 'qa058-wpf-era'
if(-not (Test-Path -LiteralPath $taskPrivatePack)) {New-Item -ItemType Directory -Path $taskPrivatePack | Out-Null}
Assert-OrdinaryPath $taskPrivatePack
foreach($taskName in @('0A','0B')) {
    $taskFrom=Join-Path $taskPack $taskName
    $taskTo=Join-Path $taskPrivatePack $taskName
    if(-not (Test-Path -LiteralPath $taskTo)) {
        $taskSource=[IO.File]::OpenRead($taskFrom)
        try {
            $taskOutput=[IO.File]::Open($taskTo,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
            try {$taskSource.CopyTo($taskOutput);$taskOutput.Flush($true)} finally {$taskOutput.Dispose()}
        } finally {$taskSource.Dispose()}
    }
    Assert-Hash $taskTo (Get-FileHash -LiteralPath $taskFrom -Algorithm SHA256).Hash
}
Assert-Process $taskGamePid $taskGameBirth $taskGamePath
Assert-Process $taskWorkerPid $taskWorkerBirth $taskWorkerPath
Assert-Process $taskParentPid $taskParentBirth $taskParentPath
if([Convert]::ToBase64String([IO.File]::ReadAllBytes($taskLeases[0].FullName)) -ne [Convert]::ToBase64String($taskLease)) {throw 'Play lease changed while preparing the diagnostic.'}
$taskApproval=Join-Path $taskDestination ('approval-'+$taskNonce+'.bin')
if(Test-Path -LiteralPath $taskApproval) {throw 'This exact session already has a diagnostic approval. It will not be replaced.'}
$taskToken=New-Object byte[] 72
[Array]::Copy([Text.Encoding]::ASCII.GetBytes('C27WNPA1'),0,$taskToken,0,8)
[Array]::Copy([BitConverter]::GetBytes([uint32]$taskGamePid),0,$taskToken,8,4)
[Array]::Copy([BitConverter]::GetBytes([uint64]$taskGameBirth),0,$taskToken,16,8)
[Array]::Copy([Text.Encoding]::ASCII.GetBytes($taskNonce),0,$taskToken,24,48)
$taskTemporary=Join-Path $taskDestination ('.approval-'+$taskNonce+'-'+[Guid]::NewGuid().ToString('N')+'.tmp')
$taskApprovalStream=[IO.File]::Open($taskTemporary,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
try {$taskApprovalStream.Write($taskToken,0,$taskToken.Length);$taskApprovalStream.Flush($true)} finally {$taskApprovalStream.Dispose()}
Move-Item -LiteralPath $taskTemporary -Destination $taskApproval -ErrorAction Stop
Write-Output ('Exact-session WNBA source diagnostic approved for game PID '+$taskGamePid+'. Wait at Home for Logs\game-session\wnba-source-test-'+$taskGamePid+'.log. REGISTERED means metadata registered; the loaded roster still requires read-only verification.')
$taskReport=Join-Path $taskPortable ('Logs\game-session\wnba-source-test-'+$taskGamePid+'.log')
$taskResultText=''
$taskResultDeadline=[DateTime]::UtcNow.AddSeconds(150)
$taskProgress=New-Object Windows.Forms.Form
$taskProgress.Text='WNBA source test - stay at Home / Quick Play'
$taskProgress.Width=620
$taskProgress.Height=190
$taskProgress.StartPosition='CenterScreen'
$taskProgress.ControlBox=$false
$taskLabel=New-Object Windows.Forms.Label
$taskLabel.Dock=[Windows.Forms.DockStyle]::Fill
$taskLabel.Padding=New-Object Windows.Forms.Padding(18)
$taskLabel.Text='WAITING: Keep the game at Home or Quick Play. You can return focus to the game. Do not enter Freestyle until REGISTERED appears.'
$taskProgress.Controls.Add($taskLabel)
$taskProgress.Show()
$taskTrace=Join-Path $taskPortable ('Logs\game-session\wnba-source-test-'+$taskGamePid+'.progress')
try {
    while([DateTime]::UtcNow -lt $taskResultDeadline) {
        if(Test-Path -LiteralPath $taskReport) {
            try {
                $taskObserved=[IO.File]::ReadAllText($taskReport)
                if($taskObserved.Contains('SESSION='+$taskNonce)) {$taskResultText=$taskObserved;break}
            } catch [IO.IOException] { }
        }
        if(Test-Path -LiteralPath $taskTrace) {
            try {$taskProgressText=[IO.File]::ReadAllText($taskTrace)} catch [IO.IOException] {$taskProgressText=''}
            $taskSteps=@([regex]::Matches($taskProgressText,'(?m) SESSION='+$taskNonce+' STATE=([^ ]+)') | ForEach-Object {$_.Groups[1].Value.Trim()})
            if($taskProgressText.Contains('SESSION='+$taskNonce) -and $taskSteps.Count) {
                $taskLabel.Text='Keep the game at Home or Quick Play. Do not enter Freestyle yet.'+[Environment]::NewLine+[Environment]::NewLine+'Progress: '+$taskSteps[-1]
            }
        }
        [Windows.Forms.Application]::DoEvents()
        Start-Sleep -Milliseconds 100
    }
} finally {$taskProgress.Close();$taskProgress.Dispose()}
if($taskResultText -match '(?m)^STATE=REGISTERED\r?$') {
    $taskMessage='The native private-roster metadata registered. Enter the NBA Learn2K / Freestyle team picker and leave it open for a read-only roster check. WNBA categories remain disabled. Registration alone does not prove that the roster loaded.'
} elseif($taskResultText -match '(?m)^STATE=(REFUSED|FAILED)\r?$') {
    $taskReason=([regex]::Match($taskResultText,'(?m)^REASON=(.*)$')).Groups[1].Value.Trim()
    $taskMessage='The diagnostic did not register the private roster. '+$taskReason
} else {
    $taskMessage='STILL PENDING: Do not enter Freestyle. Keep Home or Quick Play open and check the result log at '+$taskReport+'. Only proceed after STATE=REGISTERED. If no terminal result appears, this test is not ready; report the pending state.'
}
[Windows.Forms.MessageBox]::Show($taskMessage,'WNBA source-admission diagnostic',[Windows.Forms.MessageBoxButtons]::OK,[Windows.Forms.MessageBoxIcon]::Information) | Out-Null
