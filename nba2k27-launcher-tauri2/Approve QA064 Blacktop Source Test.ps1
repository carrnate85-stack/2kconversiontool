param(
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedDevExeSha256,
    [Parameter(Mandatory=$true)][ValidatePattern('^[0-9A-Fa-f]{64}$')][string]$ExpectedGameDllSha256,
    [switch]$CheckOnly
)
$ErrorActionPreference='Stop'
$taskRoot=$PSScriptRoot
$taskPortable=Join-Path $taskRoot 'Dev\Portable'
$taskExecutable=Join-Path $taskPortable 'App\2k Launcher Rust Prototype.exe'
$taskStage=Join-Path $taskRoot 'Dev\Pending\QA064\launcher.exe'
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
function Assert-SourceOnly {
    $taskSettingsFile=Join-Path $taskPortable 'Data\Rust\settings.json'
    Assert-OrdinaryPath $taskSettingsFile
    $taskSettingsInfo=Get-Item -LiteralPath $taskSettingsFile
    if($taskSettingsInfo.Length -gt 65536) {throw 'Dev settings exceed the diagnostic read bound.'}
    $taskSettings=Get-Content -LiteralPath $taskSettingsFile -Raw | ConvertFrom-Json
    if($taskSettings.unlockWnbaFreestyle -isnot [bool] -or $taskSettings.unlockWnbaFreestyle -ne $false) {
        throw 'Turn OFF Unlock WNBA Teams in Freestyle before starting a fresh Play session for this separate Blacktop source-only trial.'
    }
    $taskSourceDirectory=Join-Path $taskPortable 'Data\wnba-priority-v2'
    if(Test-Path -LiteralPath $taskSourceDirectory) {
        Assert-OrdinaryPath $taskSourceDirectory
        foreach($taskExclusiveName in @(('approval-'+$taskNonce+'.bin'),('categories-'+$taskNonce+'.bin'))) {
            if(Test-Path -LiteralPath (Join-Path $taskSourceDirectory $taskExclusiveName)) {
                throw 'This Play session already has a source/category approval. Close the game normally, keep Freestyle unlock OFF, and start a fresh QA064 Play session. No approval was replaced.'
            }
        }
    }
    $taskExistingReport=Join-Path $taskPortable ('Logs\game-session\wnba-source-test-'+$taskGamePid+'.log')
    if(Test-Path -LiteralPath $taskExistingReport) {
        throw 'This game already has a terminal native source-test result. A fresh Play session is required; no second registration will be attempted.'
    }
}
if($ExpectedDevExeSha256 -eq ('0'*64) -or $ExpectedGameDllSha256 -eq ('0'*64)) {throw 'QA064 source-only trial hashes have not been finalized. This staged script cannot authorize a native call yet.'}
Assert-Hash $taskExecutable $ExpectedDevExeSha256
Assert-Hash $taskStage $ExpectedDevExeSha256
$taskLeases=@(Get-ChildItem -LiteralPath (Join-Path $taskPortable 'Data') -Filter 'blacktop-outfit-v2-*.lease' -File)
if($taskLeases.Count -ne 1) {throw 'Expected exactly one current dev Play lease. Start QA064 dev with Play first.'}
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
    [IO.Path]::GetFileName($taskGamePath) -ine 'NBA2K27.exe') {throw 'The current game is not owned by this exact QA064 dev launcher.'}
Assert-SourceOnly
Assert-Process $taskGamePid $taskGameBirth $taskGamePath
Assert-Process $taskWorkerPid $taskWorkerBirth $taskWorkerPath
Assert-Process $taskParentPid $taskParentBirth $taskParentPath
Assert-Hash $taskGamePath $taskGameHash
Assert-Hash (Join-Path ([IO.Path]::GetDirectoryName($taskGamePath)) 'version.dll') $ExpectedGameDllSha256
Assert-Hash (Join-Path $taskPack '0A') '0C25D0839B11B4AF085F587C1BAF4E74F821CD34D2B902FC950DBC91E5123CC9'
Assert-Hash (Join-Path $taskPack '0B') 'D42B4E4DDFA03A83DF799A49A9BAEF1526A74393F4CFC44A5CE01AA2BF7CE7EE'
if($CheckOnly) {Write-Output 'Exact QA064 dev/game identities, source-only exclusivity and two-entry private pack verified. No approval, category token, or pack copy was written.';exit 0}
Add-Type -AssemblyName System.Windows.Forms
$taskAnswer=[Windows.Forms.MessageBox]::Show(
    'This separate DEV source-only diagnostic registers a private WAD with two lookup entries: Learn2K and the default NBA roster both point to the previously verified mixed NBA/WNBA payload. The default alias can affect other default-roster consumers until this game closes. Keep the Freestyle unlock toggle OFF. Go to Home or Quick Play and stay there until REGISTERED. Then enter Freestyle, return Home, and enter Blacktop player selection for a read-only roster check. No category approval is created and Blacktop categories stay disabled. This does not establish match compatibility; 53 appended players are still absent from the native player hash index. Close the game normally after testing. No installed archives are changed. Continue?',
    'Approve Blacktop source-only trial',[Windows.Forms.MessageBoxButtons]::YesNo,[Windows.Forms.MessageBoxIcon]::Warning)
if($taskAnswer -ne [Windows.Forms.DialogResult]::Yes) {Write-Output 'Cancelled. No diagnostic approval was written.';exit 0}
$taskDestination=Join-Path $taskPortable 'Data\wnba-priority-v2'
if(-not (Test-Path -LiteralPath $taskDestination)) {New-Item -ItemType Directory -Path $taskDestination | Out-Null}
Assert-OrdinaryPath $taskDestination
$taskPrivatePack=Join-Path $taskDestination 'qa064-blacktop-source'
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
Assert-SourceOnly
if([Convert]::ToBase64String([IO.File]::ReadAllBytes($taskLeases[0].FullName)) -ne [Convert]::ToBase64String($taskLease)) {throw 'Play lease changed while preparing the diagnostic.'}
$taskApproval=Join-Path $taskDestination ('approval-'+$taskNonce+'.bin')
if(Test-Path -LiteralPath $taskApproval) {throw 'This exact session already has a diagnostic approval. It will not be replaced.'}
$taskToken=New-Object byte[] 72
[Array]::Copy([Text.Encoding]::ASCII.GetBytes('C27WNBT1'),0,$taskToken,0,8)
[Array]::Copy([BitConverter]::GetBytes([uint32]$taskGamePid),0,$taskToken,8,4)
[Array]::Copy([BitConverter]::GetBytes([uint64]$taskGameBirth),0,$taskToken,16,8)
[Array]::Copy([Text.Encoding]::ASCII.GetBytes($taskNonce),0,$taskToken,24,48)
$taskTemporary=Join-Path $taskDestination ('.approval-'+$taskNonce+'-'+[Guid]::NewGuid().ToString('N')+'.tmp')
$taskApprovalStream=[IO.File]::Open($taskTemporary,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::Read)
try {$taskApprovalStream.Write($taskToken,0,$taskToken.Length);$taskApprovalStream.Flush($true)} finally {$taskApprovalStream.Dispose()}
Move-Item -LiteralPath $taskTemporary -Destination $taskApproval -ErrorAction Stop
Write-Output ('Exact-session Blacktop source-only trial approved for game PID '+$taskGamePid+'. Wait at Home for Logs\game-session\wnba-source-test-'+$taskGamePid+'.log. REGISTERED proves registration only. Then enter Freestyle, return Home, enter Blacktop selection, and leave it open for a read-only roster probe.')
$taskReport=Join-Path $taskPortable ('Logs\game-session\wnba-source-test-'+$taskGamePid+'.log')
$taskResultText=''
$taskResultDeadline=[DateTime]::UtcNow.AddSeconds(150)
$taskProgress=New-Object Windows.Forms.Form
$taskProgress.Text='Blacktop source-only trial - stay at Home / Quick Play'
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
                Assert-OrdinaryPath $taskReport
                if((Get-Item -LiteralPath $taskReport).Length -gt 4096) {throw 'Native terminal report exceeds its read bound.'}
                $taskObserved=[IO.File]::ReadAllText($taskReport)
                if($taskObserved -match ('(?m)^SESSION='+$taskNonce+'\r?$') -and
                   $taskObserved -match ('(?m)^PID='+$taskGamePid+'\r?$') -and
                   $taskObserved -match '(?m)^SCHEMA=wnba-native-source-admission-test-v1\r?$' -and
                   $taskObserved -match '(?m)^CATEGORY_WRITES=0\r?$') {$taskResultText=$taskObserved;break}
            } catch [IO.IOException] { }
        }
        if(Test-Path -LiteralPath $taskTrace) {
            try {
                Assert-OrdinaryPath $taskTrace
                if((Get-Item -LiteralPath $taskTrace).Length -gt 65536) {throw 'Native progress report exceeds its read bound.'}
                $taskProgressText=[IO.File]::ReadAllText($taskTrace)
            } catch [IO.IOException] {$taskProgressText=''}
            $taskSteps=@([regex]::Matches($taskProgressText,'(?m) SESSION='+$taskNonce+' STATE=([^ ]+)') | ForEach-Object {$_.Groups[1].Value.Trim()})
            if($taskProgressText.Contains('SESSION='+$taskNonce) -and $taskSteps.Count) {
                $taskLabel.Text='Keep the game at Home or Quick Play. Do not enter Freestyle yet.'+[Environment]::NewLine+[Environment]::NewLine+'Progress: '+$taskSteps[-1]
            }
        }
        [Windows.Forms.Application]::DoEvents()
        Start-Sleep -Milliseconds 100
    }
} finally {$taskProgress.Close();$taskProgress.Dispose()}
if($taskResultText -match '(?m)^STATE=REGISTERED\r?$' -and $taskResultText -match '(?m)^REASON=Blacktop source trial') {
    $taskMessage='REGISTERED: The two-entry Blacktop source trial registered. Now enter the NBA Learn2K / Freestyle team picker, return to Home, then enter Blacktop player selection and leave it open. Tell the chat you are at Blacktop selection so the loaded roster can be read-only checked. No category controls were enabled. Registration alone does not prove the roster loaded.'
} elseif($taskResultText -match '(?m)^STATE=(REFUSED|FAILED)\r?$') {
    $taskReason=([regex]::Match($taskResultText,'(?m)^REASON=(.*)$')).Groups[1].Value.Trim()
    $taskMessage='The diagnostic did not register the private roster. '+$taskReason
} else {
    $taskMessage='STILL PENDING: Do not enter Freestyle. Keep Home or Quick Play open and check the result log at '+$taskReport+'. Only proceed after STATE=REGISTERED. If no terminal result appears, this test is not ready; report the pending state.'
}
[Windows.Forms.MessageBox]::Show($taskMessage,'Blacktop source-only diagnostic',[Windows.Forms.MessageBoxButtons]::OK,[Windows.Forms.MessageBoxIcon]::Information) | Out-Null
