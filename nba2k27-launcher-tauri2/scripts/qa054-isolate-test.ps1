$ErrorActionPreference = 'Stop'
$taskRoot = Split-Path $PSScriptRoot -Parent
$taskExe = Join-Path $taskRoot 'target-qa054/debug/deps/nba2k27_launcher_tauri2-281ce2d259e8f3fd.exe'
$taskOutput = Join-Path $taskRoot 'artifacts/qa054-runtime-affinity-isolation'
New-Item -ItemType Directory -Force $taskOutput | Out-Null
$taskRows = @()
foreach ($taskCase in @('roster_v2_readonly::tests::v2_documents_feed_the_uniform_planner_end_to_end_with_oracle_bytes','roster_v2_services::tests::every_staff_control_round_trips_inside_wpf_masks_and_ranges')) {
    foreach ($taskMask in @(4294967295L,4278190080L)) {
        for ($taskRepeat = 1; $taskRepeat -le 3; $taskRepeat++) {
            $taskName = ($taskCase.Split(':')[-1])+'-'+$taskMask+'-'+$taskRepeat
            $taskClock = [Diagnostics.Stopwatch]::StartNew()
            $taskProcess = Start-Process -FilePath $taskExe -ArgumentList $taskCase,'--exact','--nocapture','--test-threads=1' -WorkingDirectory $taskRoot -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $taskOutput ($taskName+'.out.log')) -RedirectStandardError (Join-Path $taskOutput ($taskName+'.err.log'))
            $null = $taskProcess.Handle
            $taskObserved = $null
            if (-not $taskProcess.HasExited) {
                $taskProcess.ProcessorAffinity = [IntPtr]$taskMask
                $taskObserved = $taskProcess.ProcessorAffinity.ToInt64()
            }
            $taskProcess.WaitForExit()
            $taskClock.Stop()
            $taskRow = [pscustomobject]@{test=$taskCase;repeat=$taskRepeat;requestedAffinity=$taskMask;observedAffinity=$taskObserved;exitCode=$taskProcess.ExitCode;elapsedSeconds=$taskClock.Elapsed.TotalSeconds}
            $taskRows += $taskRow
            $taskRow | ConvertTo-Json -Compress
            $taskRows | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $taskOutput 'results.json') -Encoding utf8
        }
    }
}
