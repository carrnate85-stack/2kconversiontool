param()
$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$taskTools=Split-Path $taskRoot -Parent
$env:CARGO_HOME=Join-Path $taskTools '.toolchains\rust-exporter\cargo'
$env:RUSTUP_HOME=Join-Path $taskTools '.toolchains\rust-exporter\rustup'
$env:CARGO_MANIFEST_DIR=$taskRoot
$taskRefs=@()
foreach($taskDep in @('anyhow','sha2','windows')){
 $taskLib=Get-ChildItem (Join-Path $taskRoot 'target\debug\deps') -Filter ('lib'+$taskDep+'-*.rlib') | Sort-Object LastWriteTime -Descending | Select-Object -First 1
 if(-not $taskLib){throw ('Missing dependency '+$taskDep)}
 $taskRefs+=@('--extern',($taskDep+'='+$taskLib.FullName))
}
$taskRefs+=@('--extern',('serde='+ (Join-Path $taskRoot 'target\debug\deps\libserde-db8d23f55b6f3142.rlib')),
 '--extern',('serde_json='+ (Join-Path $taskRoot 'target\debug\deps\libserde_json-97ea2cefd6def516.rlib')))
foreach($taskBuild in @(@('saved-media-publish-offline','saved_media_publish_offline'),@('saved-media-publish-main',$null))){
 $taskCfg=@();if($taskBuild[1]){$taskCfg=@('--cfg',$taskBuild[1])}
 & (Join-Path $env:CARGO_HOME 'bin\rustc.exe') --edition=2024 -D warnings @taskCfg (Join-Path $taskRoot ('test\'+$taskBuild[0]+'.rs')) -L ('dependency='+ (Join-Path $taskRoot 'target\debug\deps')) @taskRefs -o (Join-Path $taskRoot ('artifacts\'+$taskBuild[0]+'.exe'))
 if($LASTEXITCODE -ne 0){throw ('Publisher build failed: '+$taskBuild[0])}
}
# Build only. The real fixed Main executable is NEVER invoked by this script.
# Root invokes it explicitly only after reviewed fixture acceptance and approval.
