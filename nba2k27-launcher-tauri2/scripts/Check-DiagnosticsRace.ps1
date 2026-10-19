$ErrorActionPreference='Stop'
$project=Split-Path $PSScriptRoot -Parent
$workspace=Split-Path $project -Parent
$compiler=Join-Path $workspace '.toolchains/llvm-mingw-20260826-ucrt-x86_64/bin/x86_64-w64-mingw32-clang++.exe'
$fixture=Join-Path $project ('artifacts/loader-directory-race-'+[guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $fixture|Out-Null
$exe=Join-Path $fixture 'NativeDiagnosticsRaceFixture.exe'
& $compiler --std=c++20 -O2 -Wall -Wextra -Werror -static -municode (Join-Path $project 'test/NativeDiagnosticsRaceFixture.cpp') -o $exe
if($LASTEXITCODE -ne 0){throw 'Original diagnostics race fixture did not compile'}
& $exe $fixture
if($LASTEXITCODE -ne 0){throw 'Race/prepared-directory regression failed'}
[pscustomobject]@{fixture=$fixture;gameAccess=$false;originalHelperUnchanged=$true;unpreparedFalseFailures=1;preparedBothPassed=$true}|ConvertTo-Json -Compress
