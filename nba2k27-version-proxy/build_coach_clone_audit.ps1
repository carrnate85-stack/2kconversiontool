param([switch]$SameSize,[switch]$AllScale)
$ErrorActionPreference='Stop'
$coachCompiler=Join-Path $PSScriptRoot '..\.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
$coachOutput=Join-Path $PSScriptRoot 'dist-coach-clone'
$coachFlags=@()
if($SameSize){$coachOutput=Join-Path $PSScriptRoot 'dist-coach-rebuild';$coachFlags=@('-DNBA2K27_COACH_SAME_SIZE_REBUILD')}
if($AllScale){$SameSize=$true;$coachOutput=Join-Path $PSScriptRoot 'dist-coach-all-scale';$coachFlags=@('-DNBA2K27_COACH_SAME_SIZE_REBUILD','-DNBA2K27_COACH_ALL_SCALE')}
New-Item -ItemType Directory -Force -Path $coachOutput | Out-Null
& $coachCompiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' @coachFlags (Join-Path $PSScriptRoot 'src\coach_clone_test.cpp') '-o' (Join-Path $coachOutput 'CoachCloneTest.exe')
if($LASTEXITCODE -ne 0){throw 'Coach policy compilation failed'}
& (Join-Path $coachOutput 'CoachCloneTest.exe')
if($LASTEXITCODE -ne 0){throw 'Coach policy tests failed'}
& $coachCompiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' (Join-Path $PSScriptRoot 'src\coach_capture_probe.cpp') '-o' (Join-Path $coachOutput 'CoachCaptureProbe.exe')
if($LASTEXITCODE -ne 0){throw 'Read-only capture probe compilation failed'}
if($SameSize){
 & $coachCompiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' (Join-Path $PSScriptRoot 'src\character_same_outfit_test.cpp') '-o' (Join-Path $coachOutput 'QueuePolicyTest.exe')
 if($LASTEXITCODE -ne 0){throw 'Queue policy compilation failed'}
 & (Join-Path $coachOutput 'QueuePolicyTest.exe')
 if($LASTEXITCODE -ne 0){throw 'Queue policy tests failed'}
}
& $coachCompiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-shared' '-static' '-DNBA2K27_COACH_CLONE_AUDIT' @coachFlags '-Wl,--no-insert-timestamp' (Join-Path $PSScriptRoot 'src\version_proxy.cpp') (Join-Path $PSScriptRoot 'src\provider_resolver.cpp') (Join-Path $PSScriptRoot 'src\version_proxy.def') '-lbcrypt' '-o' (Join-Path $coachOutput 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Coach audit compilation failed'}
& (Join-Path $PSScriptRoot 'dist\VersionProxySmokeTest.exe') (Join-Path $coachOutput 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Forwarding smoke test failed'}
Get-FileHash -LiteralPath (Join-Path $coachOutput 'version.dll') -Algorithm SHA256
