param([switch]$Isolated)
$ErrorActionPreference='Stop'
$compiler=Join-Path $PSScriptRoot '..\.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
$output=Join-Path $PSScriptRoot 'dist-session-outfits'
$extraFlags=@()
if($Isolated){$output=Join-Path $PSScriptRoot 'dist-session-outfits-isolated';$extraFlags=@('-DNBA2K27_SESSION_OUTFITS_ONLY')}
New-Item -ItemType Directory -Force -Path $output | Out-Null
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' (Join-Path $PSScriptRoot 'src\character_saved_plan_test.cpp') '-o' (Join-Path $output 'SavedPlanTest.exe')
if($LASTEXITCODE -ne 0){throw 'Saved plan test build failed'}
& (Join-Path $output 'SavedPlanTest.exe')
if($LASTEXITCODE -ne 0){throw 'Saved plan tests failed'}
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-shared' '-static' '-DNBA2K27_SESSION_OUTFITS' @extraFlags '-Wl,--no-insert-timestamp' (Join-Path $PSScriptRoot 'src\version_proxy.cpp') (Join-Path $PSScriptRoot 'src\provider_resolver.cpp') (Join-Path $PSScriptRoot 'src\version_proxy.def') '-lbcrypt' '-o' (Join-Path $output 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Session outfit build failed'}
& (Join-Path $PSScriptRoot 'dist\VersionProxySmokeTest.exe') (Join-Path $output 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Forwarding test failed'}
Get-FileHash (Join-Path $output 'version.dll')
