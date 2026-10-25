param([switch]$HighschoolDigit)
$ErrorActionPreference='Stop'
$compiler=Join-Path $PSScriptRoot '..\.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
$output=Join-Path $PSScriptRoot $(if($HighschoolDigit){'dist-highschool-digit'}else{'dist-scoreboard-text'})
$extraFlags=@();if($HighschoolDigit){$extraFlags+= '-DNBA2K27_HIGHSCHOOL_DIGIT_TEST'}
New-Item -ItemType Directory -Force -Path $output | Out-Null
foreach($test in @('selector_override_test','scoreboard_text_override_test','character_saved_plan_test','character_description_clone_test','freestyle_build_profile_test','session_diagnostics_test')){
 & $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' (Join-Path $PSScriptRoot "src\$test.cpp") '-o' (Join-Path $output "$test.exe")
 if($LASTEXITCODE -ne 0){throw "Test build failed: $test"}
 & (Join-Path $output "$test.exe")
 if($LASTEXITCODE -ne 0){throw "Test failed: $test"}
}
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-shared' '-static' '-DNBA2K27_SESSION_OUTFITS' '-DNBA2K27_SCOREBOARD_TEXT_TEST' @extraFlags '-Wl,--no-insert-timestamp' (Join-Path $PSScriptRoot 'src\version_proxy.cpp') (Join-Path $PSScriptRoot 'src\provider_resolver.cpp') (Join-Path $PSScriptRoot 'src\version_proxy.def') '-lbcrypt' '-o' (Join-Path $output 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Scoreboard text test build failed'}
& (Join-Path $PSScriptRoot 'dist\VersionProxySmokeTest.exe') (Join-Path $output 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Forwarding test failed'}
Get-FileHash (Join-Path $output 'version.dll')
