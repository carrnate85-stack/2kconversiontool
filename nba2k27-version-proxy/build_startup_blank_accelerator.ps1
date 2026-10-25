param()
$ErrorActionPreference='Stop'
$compiler=Join-Path $PSScriptRoot '..\.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
$output=Join-Path $PSScriptRoot 'dist-startup-blank-accelerator'
New-Item -ItemType Directory -Force -Path $output | Out-Null
$target=Join-Path $output 'NBA2K27StartupBlankAccelerator.exe'
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-municode' '-static' '-Wl,--no-insert-timestamp' (Join-Path $PSScriptRoot 'src\startup_blank_accelerator_sep11.cpp') '-o' $target
if($LASTEXITCODE -ne 0){throw 'Startup blank accelerator build failed'}
& $target '--self-test'
if($LASTEXITCODE -ne 0){throw 'Startup blank accelerator self-test failed'}
Get-FileHash $target
