param([switch]$CompileOnly)
$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$taskTools=Split-Path $taskRoot -Parent
$env:CARGO_HOME=Join-Path $taskTools '.toolchains\rust-exporter\cargo'
$env:RUSTUP_HOME=Join-Path $taskTools '.toolchains\rust-exporter\rustup'
$taskBackend=Get-Content -Raw (Join-Path $taskRoot 'src\backend.rs')
$taskFixture=Get-Content -Raw (Join-Path $taskRoot 'test\settings-compat-offline-harness.rs')
$taskStart=$taskBackend.IndexOf('#[derive(Clone, Serialize, Deserialize)]')
$taskEnd=$taskBackend.IndexOf('#[derive(Clone, Serialize, Deserialize)]',$taskStart+1)
$taskSettings=$taskBackend.Substring($taskStart,$taskEnd-$taskStart)
$taskStart=$taskBackend.IndexOf('fn validate_settings(')
$taskEnd=$taskBackend.IndexOf('pub fn validate_outfit(',$taskStart+1)
$taskValidate='pub '+$taskBackend.Substring($taskStart,$taskEnd-$taskStart)
$taskCompactFixture=[regex]::Replace($taskFixture,'\s+','')
foreach($taskSource in @($taskSettings,$taskValidate)) {
 if(-not $taskCompactFixture.Contains([regex]::Replace($taskSource,'\s+',''))) {throw 'Settings fixture drifted from production backend Settings/defaults/validator. Review and regenerate before testing.'}
}
$taskReferences=@()
foreach($taskDependency in @('anyhow','sha2','windows')) {
 $taskLibrary=Get-ChildItem (Join-Path $taskRoot 'target\debug\deps') -Filter ('lib'+$taskDependency+'-*.rlib') | Sort-Object LastWriteTime -Descending | Select-Object -First 1
 if(-not $taskLibrary){throw ('Missing prebuilt dependency: '+$taskDependency)}
 $taskReferences+=@('--extern',($taskDependency+'='+$taskLibrary.FullName))
}
# This pair belongs to the same existing production serde_core artifact.
# Selecting each independently by mtime can mix two incompatible build graphs.
$taskReferences+=@('--extern',('serde_json='+ (Join-Path $taskRoot 'target\debug\deps\libserde_json-97ea2cefd6def516.rlib')),
 '--extern',('serde='+ (Join-Path $taskRoot 'target\debug\deps\libserde-db8d23f55b6f3142.rlib')))
$taskOutput=Join-Path $taskRoot 'artifacts\settings-compat-offline-harness.exe'
& (Join-Path $env:CARGO_HOME 'bin\rustc.exe') --edition=2024 --cfg settings_compat_offline -D warnings (Join-Path $taskRoot 'test\settings-compat-offline-harness.rs') -L ('dependency='+ (Join-Path $taskRoot 'target\debug\deps')) @taskReferences -o $taskOutput
if($LASTEXITCODE -ne 0){throw 'Settings standalone build failed'}
if(-not $CompileOnly) {
 Push-Location $taskRoot
 try { & $taskOutput; if($LASTEXITCODE -ne 0){throw 'Settings compatibility regressions failed'} }
 finally { Pop-Location }
}
