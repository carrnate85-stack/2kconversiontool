param([switch]$EditCopy,[switch]$RebuildSame,[switch]$ChangeTop,[switch]$ExpandCopy,[switch]$FullOutfit,[switch]$SavedOutfits)
$ErrorActionPreference='Stop'
if($SavedOutfits){$FullOutfit=$true}
if($ChangeTop){$RebuildSame=$true}
if($ExpandCopy -and ($EditCopy -or $RebuildSame)){throw 'Choose one diagnostic variant'}
if($EditCopy -and $RebuildSame){throw 'Choose one diagnostic variant'}
$compiler=Join-Path $PSScriptRoot '..\.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin\x86_64-w64-mingw32-clang++.exe'
$output=Join-Path $PSScriptRoot 'dist-description-clone'
$extraFlags=@()
if ($EditCopy) { $output=Join-Path $PSScriptRoot 'dist-description-edit'; $extraFlags=@('-DNBA2K27_DESCRIPTION_CLONE_EDIT') }
if ($RebuildSame) { $output=Join-Path $PSScriptRoot 'dist-same-outfit-rebuild'; $extraFlags=@('-DNBA2K27_SAME_OUTFIT_REBUILD') }
if ($ChangeTop) { $output=Join-Path $PSScriptRoot 'dist-change-top-rebuild'; $extraFlags+=@('-DNBA2K27_CHANGE_TOP_REBUILD') }
if ($ExpandCopy) { $output=Join-Path $PSScriptRoot 'dist-expand-copy'; $extraFlags=@('-DNBA2K27_EXPAND_COPY') }
if ($FullOutfit) {
 if($EditCopy -or $RebuildSame -or $ExpandCopy){throw 'Choose one diagnostic variant'}
 $output=Join-Path $PSScriptRoot 'dist-full-outfit'
 $extraFlags=@('-DNBA2K27_EXPAND_COPY','-DNBA2K27_SAME_OUTFIT_REBUILD','-DNBA2K27_FULL_OUTFIT_REBUILD')
 $RebuildSame=$true
}
New-Item -ItemType Directory -Force -Path $output | Out-Null
if($SavedOutfits){$output=Join-Path $PSScriptRoot 'dist-saved-outfits';$extraFlags+=@('-DNBA2K27_SAVED_OUTFIT_PLAN');New-Item -ItemType Directory -Force -Path $output | Out-Null}
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' (Join-Path $PSScriptRoot 'src\character_description_clone_test.cpp') '-o' (Join-Path $output 'DescriptionCloneTest.exe')
if($LASTEXITCODE -ne 0){throw 'Clone test compilation failed'}
& (Join-Path $output 'DescriptionCloneTest.exe')
if($LASTEXITCODE -ne 0){throw 'Clone tests failed'}
if($SavedOutfits){
 & $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' (Join-Path $PSScriptRoot 'src\character_saved_plan_test.cpp') '-o' (Join-Path $output 'SavedPlanTest.exe')
 if($LASTEXITCODE -ne 0){throw 'Saved plan test compilation failed'}
 & (Join-Path $output 'SavedPlanTest.exe')
 if($LASTEXITCODE -ne 0){throw 'Saved plan test failed'}
}
if($RebuildSame) {
    & $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-static' (Join-Path $PSScriptRoot 'src\character_same_outfit_test.cpp') '-o' (Join-Path $output 'SameOutfitPolicyTest.exe')
    if($LASTEXITCODE -ne 0){throw 'Same-outfit policy compilation failed'}
    & (Join-Path $output 'SameOutfitPolicyTest.exe')
    if($LASTEXITCODE -ne 0){throw 'Same-outfit policy tests failed'}
}
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-shared' '-static' '-DNBA2K27_DESCRIPTION_CLONE_AUDIT' @extraFlags '-Wl,--no-insert-timestamp' (Join-Path $PSScriptRoot 'src\version_proxy.cpp') (Join-Path $PSScriptRoot 'src\provider_resolver.cpp') (Join-Path $PSScriptRoot 'src\version_proxy.def') '-lbcrypt' '-o' (Join-Path $output 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Clone audit proxy compilation failed'}
& (Join-Path $PSScriptRoot 'dist\VersionProxySmokeTest.exe') (Join-Path $output 'version.dll')
if($LASTEXITCODE -ne 0){throw 'Version forwarding smoke test failed'}
Get-FileHash -LiteralPath (Join-Path $output 'version.dll') -Algorithm SHA256
& (Join-Path $PSScriptRoot 'test_character_queue_restore.ps1')
