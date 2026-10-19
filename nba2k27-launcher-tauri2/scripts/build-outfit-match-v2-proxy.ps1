param()
$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$taskToolRoot=Split-Path $taskRoot -Parent
$taskCompiler=Join-Path $taskToolRoot '.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
$taskProxy=Join-Path $taskToolRoot 'nba2k27-version-proxy'
$taskOutput=Join-Path $taskRoot 'artifacts\outfit-match-v2-proxy'
New-Item -ItemType Directory -Force -Path $taskOutput | Out-Null
$taskFlags=@('--std=c++20','-O2','-Wall','-Wextra','-Werror','-static','-D_WIN32_WINNT=0x0602')
foreach($taskPureTest in @('selector_override_test','scoreboard_text_override_test','audio_index_override_test','freestyle_audio_test','freestyle_build_profile_test')) {
 & $taskCompiler @taskFlags (Join-Path $taskProxy "src\$taskPureTest.cpp") '-o' (Join-Path $taskOutput "$taskPureTest.exe")
 if($LASTEXITCODE -ne 0){throw "Retained native feature test compile failed: $taskPureTest"}
 & (Join-Path $taskOutput "$taskPureTest.exe")
 if($LASTEXITCODE -ne 0){throw "Retained native feature test failed: $taskPureTest"}
}
# Rust-owned V2 Freestyle build profile (WPF's freestyle_build_profile.hpp stays frozen).
& $taskCompiler @taskFlags '-I' (Join-Path $taskRoot 'native') (Join-Path $taskRoot 'test\freestyle-build-profile-v2-test.cpp') '-o' (Join-Path $taskOutput 'freestyle-build-profile-v2-test.exe')
if($LASTEXITCODE -ne 0){throw 'V2 Freestyle build profile test did not compile'}
& (Join-Path $taskOutput 'freestyle-build-profile-v2-test.exe')
if($LASTEXITCODE -ne 0){throw 'V2 Freestyle build profile test failed'}
& $taskCompiler @taskFlags '-municode' (Join-Path $taskRoot 'test\outfit-match-v2-native-test.cpp') '-lbcrypt' '-ladvapi32' '-o' (Join-Path $taskOutput 'outfit-match-v2-native-test.exe')
if($LASTEXITCODE -ne 0){throw 'V2 native worker tests did not compile'}
$taskFixture=New-Item -ItemType Directory -Path (Join-Path $taskOutput ('fixture-'+[guid]::NewGuid().ToString('N')))
& (Join-Path $taskOutput 'outfit-match-v2-native-test.exe') $taskFixture.FullName
if($LASTEXITCODE -ne 0){throw 'V2 native worker regression failed'}
# No NBA2K27_SESSION_OUTFITS macro. Preserve the old production DLL verbatim.
& $taskCompiler @taskFlags '-shared' '-DNBA2K27_SCOREBOARD_TEXT_TEST' '-Wl,--no-insert-timestamp' '-I' (Join-Path $taskRoot 'native') '-I' (Join-Path $taskProxy 'src') (Join-Path $taskRoot 'native\outfit-match-v2-proxy\version_proxy_v2.cpp') (Join-Path $taskProxy 'src\provider_resolver.cpp') (Join-Path $taskProxy 'src\version_proxy.def') '-lbcrypt' '-ladvapi32' '-o' (Join-Path $taskOutput 'version-outfits-v2.dll')
if($LASTEXITCODE -ne 0){throw 'Isolated V2 core proxy failed to build'}
& (Join-Path $taskProxy 'dist\VersionProxySmokeTest.exe') (Join-Path $taskOutput 'version-outfits-v2.dll')
if($LASTEXITCODE -ne 0){throw 'Isolated V2 system version forwarding smoke failed'}
$taskAsset=Join-Path $taskRoot 'assets\native'
New-Item -ItemType Directory -Force -Path $taskAsset | Out-Null
Copy-Item -LiteralPath (Join-Path $taskOutput 'version-outfits-v2.dll') -Destination (Join-Path $taskAsset 'version-outfits-v2.dll')
Get-FileHash (Join-Path $taskOutput 'version-outfits-v2.dll')
