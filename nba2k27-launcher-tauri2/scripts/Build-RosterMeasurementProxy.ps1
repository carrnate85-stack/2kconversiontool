# Build/test the dev appearance payload without installing it into a game.
$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$taskToolRoot=Split-Path $taskRoot -Parent
$taskCompiler=Join-Path $taskToolRoot '.toolchains/llvm-mingw-20260826-ucrt-x86_64/bin/x86_64-w64-mingw32-clang++.exe'
$taskReadobj=Join-Path (Split-Path $taskCompiler -Parent) 'llvm-readobj.exe'
$taskProxy=Join-Path $taskToolRoot 'nba2k27-version-proxy'
$taskOutput=Join-Path $taskRoot 'artifacts/roster-measurement-native-20261025'
New-Item -ItemType Directory -Force -Path $taskOutput | Out-Null
$taskFlags=@('--std=c++20','-O2','-Wall','-Wextra','-Werror','-static','-D_WIN32_WINNT=0x0602','-DC27_APPEARANCE_SEH=1')
$taskCasObject=Join-Path $taskOutput 'roster-appearance-v2-cas.o'
& $taskCompiler '--std=c++20' '-O0' '-Wall' '-Wextra' '-Werror' '-D_WIN32_WINNT=0x0602' '-fms-extensions' '-Xclang' '-fasync-exceptions' '-DC27_APPEARANCE_SEH=1' '-c' (Join-Path $taskRoot 'native/roster_appearance_v2_cas.cpp') '-o' $taskCasObject
if($LASTEXITCODE -ne 0){throw 'CAS compile failed'}
foreach($name in @('roster-appearance-v2-core-test','roster-appearance-v2-seh-test')) {
    $taskTest=Join-Path $taskOutput "$name.exe"
    & $taskCompiler @taskFlags (Join-Path $taskRoot "test/$name.cpp") $taskCasObject '-lbcrypt' '-ladvapi32' '-o' $taskTest
    if($LASTEXITCODE -ne 0){throw "Compile failed: $name"}
    & $taskTest
    if($LASTEXITCODE -ne 0){throw "Fixture failed: $name"}
}
$taskDll=Join-Path $taskOutput 'version-roster-measurements-20261025.dll'
& $taskCompiler @taskFlags '-shared' '-DNBA2K27_SCOREBOARD_TEXT_TEST' '-Wl,--no-insert-timestamp' '-I' (Join-Path $taskRoot 'native') '-I' (Join-Path $taskProxy 'src') (Join-Path $taskRoot 'native/outfit-match-v2-proxy/version_proxy_v2.cpp') $taskCasObject (Join-Path $taskProxy 'src/provider_resolver.cpp') (Join-Path $taskProxy 'src/version_proxy.def') '-lbcrypt' '-ladvapi32' '-o' $taskDll
if($LASTEXITCODE -ne 0){throw 'Proxy compile failed'}
& $taskReadobj '--unwind' $taskDll | Set-Content -LiteralPath (Join-Path $taskOutput 'payload-unwind.txt')
$taskUnwind=Get-Content -Raw -LiteralPath (Join-Path $taskOutput 'payload-unwind.txt')
if($taskUnwind -notmatch '(?s)StartAddress: [^\r\n]*cas32(?:(?!RuntimeFunction).)*ExceptionHandler'){throw 'CAS exception handler is absent'}
& (Join-Path $taskProxy 'dist/VersionProxySmokeTest.exe') $taskDll
if($LASTEXITCODE -ne 0){throw 'Proxy load/export smoke failed'}
Copy-Item -LiteralPath $taskDll -Destination (Join-Path $taskRoot 'assets/native/version-roster-measurements-20261025.dll')
Get-FileHash -LiteralPath $taskDll
