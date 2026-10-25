param([switch]$EditCopy,[switch]$ExpandCopy,[switch]$RebuildSame,[switch]$FullOutfit)
$ErrorActionPreference='Stop'
$compiler=Join-Path $PSScriptRoot '..\.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
$output=Join-Path $PSScriptRoot 'dist-description-clone-sep11'
$extraFlags=@()
if($EditCopy -and $ExpandCopy){throw 'Choose one private-copy test'}
if($FullOutfit -and ($EditCopy -or $ExpandCopy -or $RebuildSame)){throw 'Choose one diagnostic test'}
if($RebuildSame -and ($EditCopy -or $ExpandCopy)){throw 'Choose one diagnostic test'}
if($RebuildSame){$output=Join-Path $PSScriptRoot 'dist-same-outfit-sep11';$extraFlags=@('-DNBA2K27_SAME_OUTFIT_REBUILD')}
if($ExpandCopy){$output=Join-Path $PSScriptRoot 'dist-expand-copy-sep11';$extraFlags=@('-DNBA2K27_EXPAND_COPY')}
if($EditCopy){$output=Join-Path $PSScriptRoot 'dist-description-edit-sep11';$extraFlags=@('-DNBA2K27_DESCRIPTION_CLONE_EDIT')}
if($FullOutfit){
 $output=Join-Path $PSScriptRoot 'dist-full-outfit-sep11'
 $extraFlags=@('-DNBA2K27_EXPAND_COPY','-DNBA2K27_SAME_OUTFIT_REBUILD','-DNBA2K27_FULL_OUTFIT_REBUILD')
 $RebuildSame=$true
}
New-Item -ItemType Directory -Force -Path $output | Out-Null
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' (Join-Path $PSScriptRoot 'src\character_description_clone_test.cpp') '-o' (Join-Path $output 'DescriptionCloneTest.exe')
if($LASTEXITCODE -ne 0){throw 'Clone policy build failed'}
& (Join-Path $output 'DescriptionCloneTest.exe')
if($LASTEXITCODE -ne 0){throw 'Clone policy tests failed'}
if($RebuildSame){
 & $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' (Join-Path $PSScriptRoot 'src\character_same_outfit_test.cpp') '-o' (Join-Path $output 'SameOutfitPolicyTest.exe')
 if($LASTEXITCODE -ne 0){throw 'Rebuild policy compilation failed'}
 & (Join-Path $output 'SameOutfitPolicyTest.exe')
 if($LASTEXITCODE -ne 0){throw 'Rebuild policy tests failed'}
}
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-shared' '-static' '-DNBA2K27_DESCRIPTION_CLONE_AUDIT' '-DNBA2K27_DESCRIPTION_CLONE_SEP11' @extraFlags '-Wl,--no-insert-timestamp' (Join-Path $PSScriptRoot 'src\version_proxy.cpp') (Join-Path $PSScriptRoot 'src\provider_resolver.cpp') (Join-Path $PSScriptRoot 'src\version_proxy.def') '-lbcrypt' '-o' (Join-Path $output 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Private-copy diagnostic build failed'}
& (Join-Path $PSScriptRoot 'dist\VersionProxySmokeTest.exe') (Join-Path $output 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Forwarding smoke test failed'}
Get-FileHash -LiteralPath (Join-Path $output 'version.dll') -Algorithm SHA256
