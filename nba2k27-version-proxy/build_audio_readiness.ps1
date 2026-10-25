$ErrorActionPreference='Stop'
$compiler=Join-Path $PSScriptRoot '..\.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
$readiness=Join-Path $PSScriptRoot 'dist\NBA2K27AudioReadiness.exe'
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-municode' '-static' (Join-Path $PSScriptRoot 'src\audio_readiness.cpp') '-o' $readiness
if($LASTEXITCODE -ne 0){throw 'Audio readiness build failed'}
& $readiness '--self-test'
if($LASTEXITCODE -ne 0){throw 'Audio readiness self-test failed'}
