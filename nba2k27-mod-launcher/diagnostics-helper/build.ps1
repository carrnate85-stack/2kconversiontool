$ErrorActionPreference = 'Stop'
$workspace = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$compiler = Join-Path $workspace '.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
New-Item -ItemType Directory -Force -Path (Join-Path $PSScriptRoot 'dist') | Out-Null
& $compiler '-std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' '-static-libgcc' '-static-libstdc++' '-municode' '-Wl,--no-insert-timestamp' '-o' (Join-Path $PSScriptRoot 'dist\NBA2K27GameCapture.exe') (Join-Path $PSScriptRoot 'GameCapture.cpp') '-ldbghelp' '-luser32'
if ($LASTEXITCODE -ne 0) { throw 'Game capture helper compilation failed.' }
