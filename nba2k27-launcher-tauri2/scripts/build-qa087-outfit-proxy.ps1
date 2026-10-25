$ErrorActionPreference='Stop'
$taskRoot=Split-Path $PSScriptRoot -Parent
$taskToolRoot=Split-Path $taskRoot -Parent
$taskCompiler=Join-Path $taskToolRoot '.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
$taskReadobj=Join-Path (Split-Path $taskCompiler -Parent) 'llvm-readobj.exe'
$taskProxy=Join-Path $taskToolRoot 'nba2k27-version-proxy'
$taskOutput=Join-Path $taskRoot 'artifacts\qa087-native-payload-20261021'
New-Item -ItemType Directory -Force -Path $taskOutput|Out-Null
$taskFlags=@('--std=c++20','-O2','-Wall','-Wextra','-Werror','-static','-D_WIN32_WINNT=0x0602','-DC27_APPEARANCE_SEH=1')
$taskCasObject=Join-Path $taskOutput 'roster-appearance-v2-cas.o'
& $taskCompiler '--std=c++20' '-O0' '-Wall' '-Wextra' '-Werror' '-D_WIN32_WINNT=0x0602' '-fms-extensions' '-Xclang' '-fasync-exceptions' '-DC27_APPEARANCE_SEH=1' '-c' (Join-Path $taskRoot 'native\roster_appearance_v2_cas.cpp') '-o' $taskCasObject
if($LASTEXITCODE -ne 0){throw 'Isolated asynchronous POD CAS compile failed'}
function Invoke-OwnedFixture([string]$path,[string[]]$arguments=@()) {
 $info=[Diagnostics.ProcessStartInfo]::new($path);$info.UseShellExecute=$false;$info.CreateNoWindow=$true;$info.RedirectStandardOutput=$true;$info.RedirectStandardError=$true
 foreach($argument in $arguments){$info.ArgumentList.Add($argument)}
 $child=[Diagnostics.Process]::Start($info)
 if(!$child.WaitForExit(30000)){$child.Kill();$child.WaitForExit(5000)|Out-Null;throw "Owned native fixture exceeded30seconds: $path"}
 $stdout=$child.StandardOutput.ReadToEnd();$stderr=$child.StandardError.ReadToEnd();[IO.File]::WriteAllText((Join-Path $taskOutput ((Split-Path $path -Leaf)+'.run.log')),$stdout+$stderr)
 if($child.ExitCode -ne 0){throw "Native fixture failed($($child.ExitCode)): $path"}
 Write-Output ('PASS '+(Split-Path $path -Leaf))
}
foreach($test in @('selector_override_test','scoreboard_text_override_test','audio_index_override_test','freestyle_audio_test','freestyle_build_profile_test')){
 & $taskCompiler @taskFlags (Join-Path $taskProxy "src\$test.cpp") '-o' (Join-Path $taskOutput "$test.exe")
 if($LASTEXITCODE -ne 0){throw "Native compile failed: $test"};Invoke-OwnedFixture (Join-Path $taskOutput "$test.exe")
}
foreach($test in @('freestyle-build-profile-v2-test','roster-appearance-v2-core-test')){
 & $taskCompiler @taskFlags '-I' (Join-Path $taskRoot 'native') (Join-Path $taskRoot "test\$test.cpp") '-o' (Join-Path $taskOutput "$test.exe")
 if($LASTEXITCODE -ne 0){throw "Native compile failed: $test"};Invoke-OwnedFixture (Join-Path $taskOutput "$test.exe")
}
$seh=Join-Path $taskOutput 'roster-appearance-v2-seh-test.exe'
& $taskCompiler @taskFlags (Join-Path $taskRoot 'test\roster-appearance-v2-seh-test.cpp') $taskCasObject '-lbcrypt' '-ladvapi32' '-o' $seh
if($LASTEXITCODE -ne 0){throw 'SEH fixture compile failed'}
& $taskReadobj '--unwind' $seh | Set-Content -LiteralPath (Join-Path $taskOutput 'seh-unwind.txt')
$unwind=Get-Content -Raw -LiteralPath (Join-Path $taskOutput 'seh-unwind.txt')
if($unwind -notmatch '(?s)StartAddress: [^\r\n]*cas32(?:(?!RuntimeFunction).)*ExceptionHandler'){throw 'cas32 must have asynchronous native exception unwind protection'}
Invoke-OwnedFixture $seh
$nativeTest=Join-Path $taskOutput 'outfit-match-v2-native-test.exe'
& $taskCompiler @taskFlags '-municode' (Join-Path $taskRoot 'test\outfit-match-v2-native-test.cpp') '-lbcrypt' '-ladvapi32' '-o' $nativeTest
if($LASTEXITCODE -ne 0){throw 'Outfit native regression compile failed'}
$fixture=Join-Path $taskOutput ('fixture-'+[guid]::NewGuid().ToString('N'));New-Item -ItemType Directory -Path $fixture|Out-Null
Invoke-OwnedFixture $nativeTest @($fixture)
$dll=Join-Path $taskOutput 'version-outfits-v2-qa087.dll'
& $taskCompiler @taskFlags '-shared' '-DNBA2K27_SCOREBOARD_TEXT_TEST' '-Wl,--no-insert-timestamp' '-I' (Join-Path $taskRoot 'native') '-I' (Join-Path $taskProxy 'src') (Join-Path $taskRoot 'native\outfit-match-v2-proxy\version_proxy_v2.cpp') $taskCasObject (Join-Path $taskProxy 'src\provider_resolver.cpp') (Join-Path $taskProxy 'src\version_proxy.def') '-lbcrypt' '-ladvapi32' '-o' $dll
if($LASTEXITCODE -ne 0){throw 'QA087 proxy compile failed'}
& $taskReadobj '--unwind' $dll | Set-Content -LiteralPath (Join-Path $taskOutput 'payload-unwind.txt')
$unwind=Get-Content -Raw -LiteralPath (Join-Path $taskOutput 'payload-unwind.txt')
if($unwind -notmatch '(?s)StartAddress: [^\r\n]*cas32(?:(?!RuntimeFunction).)*ExceptionHandler'){throw 'Final payload cas32 has no asynchronous exception handler'}
Invoke-OwnedFixture (Join-Path $taskProxy 'dist\VersionProxySmokeTest.exe') @($dll)
$asset=Join-Path $taskRoot 'assets\native\version-outfits-v2-qa087.dll'
Copy-Item -LiteralPath $dll -Destination $asset
Get-FileHash -LiteralPath $asset
