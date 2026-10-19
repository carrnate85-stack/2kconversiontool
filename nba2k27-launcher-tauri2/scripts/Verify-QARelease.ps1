param([Parameter(Mandatory)][string]$Version,[string]$MainExe,[ValidateRange(1,5)][int]$Repeats=3,[ValidateRange(0,1000)][int]$ExpectedUiChecks=0)
$ErrorActionPreference='Stop'
if($Version -notmatch '^\d+\.\d+\.\d+$'){throw 'Use a numeric QA version'}
if($ExpectedUiChecks -eq 0){$ExpectedUiChecks=if([version]$Version -ge [version]'0.35.0'){105}else{100}}
$project=(Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$build=Join-Path $project ('Builds/'+$Version)
$portable=Join-Path $build 'Portable'
$exe=Join-Path $portable 'App/2k Launcher Rust Prototype.exe'
$info=Get-Content -LiteralPath (Join-Path $portable 'BUILD-INFO.json') -Raw|ConvertFrom-Json
$evidencePath=Join-Path $build 'QA-VERIFICATION.json'
if(Test-Path -LiteralPath $evidencePath){throw 'Existing verification will not be replaced'}
if($info.version -ne $Version -or -not $info.isolatedUiVerified -or $info.uiChecks -ne $ExpectedUiChecks -or (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $info.sha256){throw 'Run the verified build pipeline first'}
if(Get-CimInstance Win32_Process|Where-Object {$_.ExecutablePath -eq $exe}){throw 'Close this QA executable before repeating tests'}
$mainBefore=if($MainExe){(Get-FileHash -LiteralPath $MainExe -Algorithm SHA256).Hash}else{$null}
# The build shell is deliberately pinned; GUI acceptance must not inherit that
# workaround. These fixtures are sequential because their F24 test key is owned.
[Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity=[IntPtr]4294967295
$repeatRows=@()
for($i=1;$i -le $Repeats;$i++){
    $name='final-v'+$Version.Replace('.','')+'-repeat-'+$i+'-'+[guid]::NewGuid().ToString('N')
    $fixture=Join-Path $project ('artifacts/'+$name)
    $process=Start-Process -FilePath $exe -ArgumentList @('--smoke','--smoke-root',('"'+$fixture+'"')) -WindowStyle Hidden -PassThru
    if(-not $process.WaitForExit(60000)){throw "Owned GUI fixture timed out: PID $($process.Id), $fixture"}
    $report=Get-Content -LiteralPath (Join-Path $fixture 'Logs/ui-smoke.json') -Raw|ConvertFrom-Json
    if($process.ExitCode -ne 0 -or -not $report.result.pass -or $report.result.gameWrites -ne $false -or $report.result.checks.Count -ne $ExpectedUiChecks){throw 'Packaged GUI repeat failed'}
    node (Join-Path $PSScriptRoot 'verify-shared-exports.mjs') $fixture
    if($LASTEXITCODE -ne 0){throw 'WPF shared-file validator rejected a repeat'}
    $row=@{root=('artifacts/'+$name);pid=$process.Id;exited=$true;checks=$report.result.checks.Count;uiReadyMs=$report.result.startupMs;wpfValidatorsPassed=$true}
    $repeatRows+=$row
    $row|ConvertTo-Json -Compress
}
$browserName='browser-wpf-v'+$Version.Replace('.','')+'-final-'+[guid]::NewGuid().ToString('N')
$browserRoot=Join-Path $project ('artifacts/'+$browserName)
$browser=Start-Process -FilePath $exe -ArgumentList @('--smoke','--smoke-root',('"'+$browserRoot+'"'),'--browser-workspace-check') -WindowStyle Hidden -PassThru
$session=Join-Path $browserRoot 'Logs/browser-workspace-session.json'
$timer=[Diagnostics.Stopwatch]::StartNew()
while(-not (Test-Path -LiteralPath $session)){
    if($browser.HasExited -or $timer.Elapsed.TotalSeconds -gt 40){throw "Owned browser fixture did not start: PID $($browser.Id)"}
    Start-Sleep -Milliseconds 100
}
try {& (Join-Path $PSScriptRoot 'Check-BrowserWorkspace.ps1') -FixtureRoot $browserRoot|Out-Null}
finally {New-Item -ItemType File -Path (Join-Path $browserRoot 'Logs/browser-check-stop') -Force|Out-Null}
if(-not $browser.WaitForExit(15000) -or $browser.ExitCode -ne 0){throw "Owned browser fixture did not exit cleanly: PID $($browser.Id)"}
$http=Get-Content -LiteralPath (Join-Path $browserRoot 'Logs/browser-http-check.json') -Raw|ConvertFrom-Json
if(-not $http.pass -or $http.teamSaveBlocked -ne 400 -or $http.uniformSaveBlocked -ne 400){throw 'No-game browser fixture failed'}
$workerName='provider-worker-v'+$Version.Replace('.','')+'-final-'+[guid]::NewGuid().ToString('N')
$workerRoot=Join-Path $project ('artifacts/'+$workerName)
$worker=Start-Process -FilePath $exe -ArgumentList @('--smoke','--smoke-root',('"'+$workerRoot+'"'),'--provider-worker-check') -WindowStyle Hidden -PassThru
if(-not $worker.WaitForExit(50000)){throw "Owned provider fixture timed out: PID $($worker.Id), $workerRoot"}
$workerReport=Get-Content -LiteralPath (Join-Path $workerRoot 'Logs/provider-worker-check.json') -Raw|ConvertFrom-Json
if($worker.ExitCode -ne 0 -or -not $workerReport.passed -or -not $workerReport.workerExited -or -not $workerReport.preparedUnstartedHelperExited -or $workerReport.gameAccess -ne $false -or $workerReport.gameFilesChanged -ne $false -or $workerReport.tokenOnCommandLine -ne $false -or $workerReport.tokenInReport -ne $false){throw 'Packaged native provider worker fixture failed'}
if([version]$Version -ge [version]'0.29.0' -and ($report.result.checks -notcontains 'live-outfit-access-refused-in-smoke' -or $report.result.checks -notcontains 'shared-outfit-preview-player-apply-restore-controls' -or $report.result.checks -notcontains 'native-overlay-outfit-requires-own-provider-and-ack')){throw 'Packaged outfit preview, ownership or no-game safety check is missing'}
$bytes=[IO.File]::ReadAllBytes($exe)
$binary=[Text.Encoding]::ASCII.GetString($bytes)
foreach($marker in @('TeamWriteFixture','UniformWriteFixture','Synthetic close during owner inspection','Synthetic uniform close cancellation','originalTeamRequestCases')){if($binary.Contains($marker)){throw 'Test-only marker found in release'}}
if((Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash -ne $info.sha256){throw 'Executable changed during QA'}
$mainAfter=if($MainExe){(Get-FileHash -LiteralPath $MainExe -Algorithm SHA256).Hash}else{$null}
if($mainBefore -ne $mainAfter){throw 'Main executable changed during QA; inspect before continuing'}
$result=[ordered]@{
    version=$Version;verifiedUtc=[DateTime]::UtcNow.ToString('o');exeSha256=$info.sha256;exeBytes=$bytes.Length
    pipelineBuildVerified=$true;pipelineUiChecks=$info.uiChecks;repeatProcessAffinity=4294967295;repeats=$repeatRows
    httpFixtureRoot=('artifacts/'+$browserName);httpFixturePid=$browser.Id;httpFixtureExited=$true;http=$http
    providerWorkerFixtureRoot=('artifacts/'+$workerName);providerWorkerFixturePid=$worker.Id;providerWorkerFixtureExited=$true;providerWorker=$workerReport
    testOnlyMarkersAbsentFromRelease=$true;mainInstalled=$false;mainExeSha256Unchanged=$mainAfter;dataAndLibraryPreserved=$true
    automatedGameAccess=$false;liveTeamSaveRouteEnabled=$true;liveTeamSaveVerified=$false;liveArenaFloorVerified=$false
    liveUniformSaveRouteEnabled=$true;liveUniformSaveVerified=$false
    liveBlacktopArenaRouteEnabled=$true;liveBlacktopArenaVerified=$false
    liveBlacktopBallRouteEnabled=$true;liveBlacktopBallVerified=$false
    liveBlacktopManualOutfitRouteEnabled=$true;liveBlacktopOutfitVerified=$false;automaticBlacktopOutfitsEnabled=$false
    nativeWriteRoutesSupportedBuildOnly=$true
    gameVersion2ProviderMapped=([version]$Version -ge [version]'0.30.0');gameVersion2ProviderLiveVerified=$false
    gameVersion2ObserverMapped=([version]$Version -ge [version]'0.30.0')
    gameVersion2LegacyMemoryWriteGateEnabled=$false
    gameVersion2ArenaBallRouteEnabled=([version]$Version -ge [version]'0.34.0');gameVersion2ArenaBallGameplayVerified=$false
    gameVersion2GameplayBallSettingRouteEnabled=([version]$Version -ge [version]'0.35.0');gameVersion2GameplayBallVisualVerified=$false
    gameVersion2ScoreboardRouteEnabled=([version]$Version -ge [version]'0.35.0');gameVersion2ScoreboardGameplayVerified=$false
    gameVersion2PresentationRouteEnabled=([version]$Version -ge [version]'0.35.0');gameVersion2PresentationGameplayVerified=$false
    gameVersion2JukeboxRouteEnabled=([version]$Version -ge [version]'0.35.0');gameVersion2JukeboxPlaybackVerified=$false
    gameVersion2OutfitCarryOverEnabled=$false
    gameVersion2RulesRouteEnabled=([version]$Version -ge [version]'0.34.0');gameVersion2RulesGameplayVerified=$false
    gameVersion2OptInFeatureDataProfiles=$(if([version]$Version -ge [version]'0.42.0'){@('cameras','player-scaler')}elseif([version]$Version -ge [version]'0.33.0'){@('cameras','player-scaler','wnba-freestyle')}else{@()})
    gameVersion2WnbaFreestyleRouteEnabled=([version]$Version -ge [version]'0.41.0' -and [version]$Version -lt [version]'0.42.0')
    gameVersion2WnbaFreestyleWithdrawnAfterCrash=([version]$Version -ge [version]'0.42.0')
    gameVersion2OptInFeaturesDefaultOff=$true;gameVersion2FeatureGameplayVerified=$false
    automatedOutfitAccessRefused=($report.result.checks -contains 'live-outfit-access-refused-in-smoke')
    sharedOutfitPreviewControlsVerified=($report.result.checks -contains 'shared-outfit-preview-player-apply-restore-controls')
    liveOutfitControlsRequireOwnedProviderAndAck=($report.result.checks -contains 'native-overlay-outfit-requires-own-provider-and-ack')
    automatedBallWritesRefused=($report.result.checks -contains 'real-ball-write-refused-in-smoke')
    automatedArenaWritesRefused=($report.result.checks -contains 'real-arena-write-refused-in-smoke');manualVisualReviewRequired=$true
    browserVisualVerified=$false;nativeDialogManualUiVerified=$false;parityComplete=$false;goalStillActive=$true
}
[IO.File]::WriteAllText($evidencePath,($result|ConvertTo-Json -Depth 10),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{verification=$evidencePath;executable=$exe;pass=$true;httpPass=$http.pass;mainUnchanged=($mainBefore -eq $mainAfter)}|ConvertTo-Json -Compress
