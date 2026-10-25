$ErrorActionPreference='Stop'
$compiler=Join-Path $PSScriptRoot '..\.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
$output=Join-Path $PSScriptRoot 'dist-character-queue-thread'
New-Item -ItemType Directory -Force -Path $output | Out-Null
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' (Join-Path $PSScriptRoot 'src\character_queue_thread_test.cpp') '-o' (Join-Path $output 'QueueThreadTest.exe')
if($LASTEXITCODE -ne 0){throw 'Thread observer test compilation failed'}
& (Join-Path $output 'QueueThreadTest.exe')
if($LASTEXITCODE -ne 0){throw 'Thread observer tests failed'}
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-shared' '-static' '-DNBA2K27_CHARACTER_QUEUE_NOOP_TEST' '-DNBA2K27_CHARACTER_QUEUE_THREAD_AUDIT' '-Wl,--no-insert-timestamp' (Join-Path $PSScriptRoot 'src\version_proxy.cpp') (Join-Path $PSScriptRoot 'src\provider_resolver.cpp') (Join-Path $PSScriptRoot 'src\version_proxy.def') '-lbcrypt' '-o' (Join-Path $output 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Thread audit proxy compilation failed'}
& (Join-Path $PSScriptRoot 'dist\VersionProxySmokeTest.exe') (Join-Path $output 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Version forwarding smoke test failed'}
Get-FileHash -LiteralPath (Join-Path $output 'version.dll') -Algorithm SHA256
& (Join-Path $PSScriptRoot 'test_character_queue_restore.ps1')
