param([ValidateSet('Before','After')][string]$Variant='After')
$ErrorActionPreference='Stop'
$project=(Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$taskArtifacts=Join-Path $project 'artifacts/player-startup-performance'
[System.IO.Directory]::CreateDirectory($taskArtifacts)|Out-Null
$player=if($Variant -eq 'Before'){Join-Path $project 'Backups/player-folders-before-startup-perf-20261016/player_folders.rs'}else{Join-Path $project 'src/player_folders.rs'}
$source=[System.IO.File]::ReadAllText((Join-Path $project 'test/selector-files-stability-harness.rs'))
$source=$source.Replace('../src/',($project.Replace('\','/')+'/src/'))
$source=$source.Replace(('#[path = "'+$project.Replace('\','/')+'/src/player_folders.rs"]'),('#[path = "'+$player.Replace('\','/')+'"]'))
$safety=[System.IO.File]::ReadAllText((Join-Path $project 'src/session_safety.rs'))
$boundary=$safety.IndexOf('fn bounded(')
if($boundary -lt 0 -or $safety.IndexOf('fn bounded(',$boundary+1) -ge 0){throw 'Cannot uniquely extract the unchanged read-only process-scan prefix'}
$safety=$safety.Substring(0,$boundary)
$scan=Join-Path $taskArtifacts "process-scan-$Variant.rs"
[System.IO.File]::WriteAllText($scan,$safety,[System.Text.UTF8Encoding]::new($false))
$source+='`r`n'.Replace('`r`n',"`r`n")+('#[path="'+$scan.Replace('\','/')+'"] mod process_guard;')+"`r`n"
$source+=[System.IO.File]::ReadAllText((Join-Path $project 'test/player-folders-startup-benchmark.rs'))
$generated=Join-Path $taskArtifacts "player-perf-$Variant.rs"
[System.IO.File]::WriteAllText($generated,$source,[System.Text.UTF8Encoding]::new($false))
$env:CARGO_HOME=Join-Path $project '../.toolchains/rust-exporter/cargo'
$env:RUSTUP_HOME=Join-Path $project '../.toolchains/rust-exporter/rustup'
$compiler=Join-Path $env:CARGO_HOME 'bin/rustc.exe'
$deps=Join-Path $project 'artifacts/jukebox-edit-api-target/debug/deps'
$globalDeps=Join-Path $project 'target/debug/deps'
$output=Join-Path $taskArtifacts "player-perf-$Variant.exe"
$arguments=@('--edition=2024','--test',$generated,'-o',$output,'-L',"dependency=$deps",'-L',"dependency=$globalDeps")
foreach($crate in @('anyhow','serde','serde_json','sha2','zip','flate2')){
  $lib=@(Get-ChildItem -LiteralPath $deps -Filter "lib$crate-*.rlib")
  if($lib.Count -ne 1){throw "Expected one isolated $crate dependency"}
  $arguments+=@('--extern',"$crate=$($lib[0].FullName)")
}
$windows=Join-Path $globalDeps 'libwindows-2edf0964ee4c3e29.rlib'
if(!(Test-Path -LiteralPath $windows -PathType Leaf)){throw 'Reviewed full Win32 dependency unavailable'}
$arguments+=@('--extern',"windows=$windows")
& $compiler @arguments
if($LASTEXITCODE -ne 0){throw 'Isolated player benchmark compile failed'}
Write-Output $output
