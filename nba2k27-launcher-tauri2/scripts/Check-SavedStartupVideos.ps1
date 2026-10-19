param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference='Stop'
if($Version -notmatch '^\d+\.\d+\.\d+$'){throw 'Use an exact QA version'}
$taskProject=(Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$taskPortable=Join-Path $taskProject ('Builds/'+$Version+'/Portable')
$taskInfo=Get-Content -LiteralPath (Join-Path $taskPortable 'BUILD-INFO.json') -Raw|ConvertFrom-Json
$taskExe=Join-Path $taskPortable 'App/2k Launcher Rust Prototype.exe'
if(-not $taskInfo.isolatedUiVerified -or $taskInfo.version -ne $Version -or (Get-FileHash -LiteralPath $taskExe -Algorithm SHA256).Hash -ne $taskInfo.sha256){throw 'Run the verified build first'}
$taskStage=Join-Path $taskProject 'artifacts/legacy-media-staging-20261016-native'
$taskCatalog=Join-Path $taskStage 'Data/Rust/startup-videos.json'
$taskCatalogSha=(Get-FileHash -LiteralPath $taskCatalog -Algorithm SHA256).Hash
if($taskCatalogSha -ne 'E223D42E0B327145493B7C58F5E790C4E6A18D8396EF6C6BE2B5F585D9852A8B'){throw 'Reviewed saved-video catalog changed'}
$taskVideos=Get-Content -LiteralPath $taskCatalog -Raw|ConvertFrom-Json
if($taskVideos.schema -ne 'nba2k27-rust-startup-video-library-v1' -or $taskVideos.items.Count -ne 4){throw 'Expected the four reviewed saved MP4s'}
$taskFixture=Join-Path $taskProject ('artifacts/saved-mp4-v'+$Version.Replace('.','')+'-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $taskFixture|Out-Null
New-Item -ItemType Directory -Path (Join-Path $taskFixture 'Data/Rust')|Out-Null
New-Item -ItemType Directory -Path (Join-Path $taskFixture 'Library/Videos/mp4')|Out-Null
Copy-Item -LiteralPath $taskCatalog -Destination (Join-Path $taskFixture 'Data/Rust/startup-videos.json')
foreach($taskVideo in $taskVideos.items){
    if($taskVideo.id -notmatch '^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$' -or $taskVideo.relativePath -ne ('Library/Videos/mp4/'+$taskVideo.id+'.mp4')){throw 'Video escaped the fixed UUID library'}
    $taskSource=Join-Path $taskStage $taskVideo.relativePath
    if((Get-Item -LiteralPath $taskSource).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Linked source refused'}
    $taskInput=[IO.File]::Open($taskSource,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
    try {
        $taskSha=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($taskInput))
        if($taskInput.Length -ne $taskVideo.bytes -or $taskSha -ne $taskVideo.sha256){throw 'Source MP4 differs from reviewed hash'}
        $taskInput.Position=0
        $taskOutput=[IO.File]::Open((Join-Path $taskFixture $taskVideo.relativePath),[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
        try {$taskInput.CopyTo($taskOutput);$taskOutput.Flush($true)} finally {$taskOutput.Dispose()}
    } finally {$taskInput.Dispose()}
    if((Get-FileHash -LiteralPath (Join-Path $taskFixture $taskVideo.relativePath) -Algorithm SHA256).Hash -ne $taskVideo.sha256){throw 'Copied MP4 hash differs'}
}
# This is a silent own-application decoding fixture, not a game launch. It
# deliberately has no game binding, audio guardian, Steam or memory route.
[Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity=[IntPtr]4294967295
$taskProcess=Start-Process -FilePath $taskExe -ArgumentList @('--smoke','--smoke-root',('"'+$taskFixture+'"'),'--startup-video-check') -WindowStyle Hidden -PassThru
$taskTimer=[Diagnostics.Stopwatch]::StartNew()
while(-not $taskProcess.HasExited -and $taskTimer.Elapsed.TotalSeconds -lt 180){Start-Sleep -Milliseconds 200;$taskProcess.Refresh()}
if(-not $taskProcess.HasExited){throw "Owned decode fixture timed out; inspect PID $($taskProcess.Id), $taskFixture; it was not killed"}
$taskReport=Get-Content -LiteralPath (Join-Path $taskFixture 'Logs/ui-smoke.json') -Raw|ConvertFrom-Json
if($taskProcess.ExitCode -ne 0 -or -not $taskReport.result.pass -or $taskReport.result.schema -ne 'nba2k27-startup-mp4-decode-v1' -or $taskReport.result.rows.Count -ne 4 -or $taskReport.result.gameAccess -ne $false -or $taskReport.result.guardianStarted -ne $false -or $taskReport.result.audioOutput -ne $false){throw "Saved MP4 decode failed: $taskFixture"}
foreach($taskRow in $taskReport.result.rows){if(-not $taskRow.passed -or $taskRow.points.Count -ne 3 -or $taskRow.width -le 0 -or $taskRow.height -le 0){throw 'A saved MP4 did not produce all three actual frames'}}
if((Get-FileHash -LiteralPath $taskCatalog -Algorithm SHA256).Hash -ne $taskCatalogSha){throw 'Source catalog changed during test'}
$taskResult=[ordered]@{version=$Version;exeSha256=$taskInfo.sha256;fixture=$taskFixture;pass=$true;decodedVideos=4;decodedFramePositions=12;audioOutput=$false;gameAccess=$false;mainChanged=$false;rows=$taskReport.result.rows}
$taskEvidence=Join-Path $taskProject ('Builds/'+$Version+'/SAVED-MP4-VERIFICATION.json')
if(Test-Path -LiteralPath $taskEvidence){throw 'Existing decode evidence will not be overwritten'}
[IO.File]::WriteAllText($taskEvidence,($taskResult|ConvertTo-Json -Depth 10),[Text.UTF8Encoding]::new($false))
$taskResult|ConvertTo-Json -Depth 10
