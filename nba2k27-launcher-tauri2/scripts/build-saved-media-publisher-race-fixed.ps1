# Build engineering-only new filenames. Never overwrites the reviewed 746656...
# original, invokes Main, launches a game or changes the verified app build.
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
$taskBuilds=@(
 @('test/saved-media-publish-guard-offline.rs','saved-media-publish-guard-offline-fixed.exe','publisher_guard_offline'),
 @('test/saved-media-publish-offline.rs','saved-media-publish-offline-fixed.exe','saved_media_publish_offline'),
 @('test/saved-media-publish-main.rs','saved-media-publish-main-race-fixed.exe',$null)
)
foreach($taskBuild in $taskBuilds){
 $taskCfg=@();if($taskBuild[2]){$taskCfg=@('--cfg',$taskBuild[2])}
 & (Join-Path $env:CARGO_HOME 'bin\rustc.exe') --edition=2024 -D warnings @taskCfg (Join-Path $taskRoot $taskBuild[0]) -L ('dependency='+ (Join-Path $taskRoot 'target\debug\deps')) @taskRefs -o (Join-Path $taskRoot ('artifacts/'+$taskBuild[1]))
 if($LASTEXITCODE -ne 0){throw ('Engineering publisher build failed: '+$taskBuild[1])}
}
if((Get-FileHash -LiteralPath (Join-Path $taskRoot 'artifacts/saved-media-publish-main.exe')).Hash -ne '746656C3B5A29AFAAB4CC72A6A7BF430532CF30319E879F2688CC8A526FEACE9'){throw 'Original frozen publisher changed'}
