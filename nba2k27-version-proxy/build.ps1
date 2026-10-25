$ErrorActionPreference = 'Stop'

$toolchain = Join-Path $PSScriptRoot '..\.toolchains\llvm-mingw-20260826-ucrt-x86_64\bin'
$compiler = Join-Path $toolchain 'x86_64-w64-mingw32-clang++.exe'
if (-not (Test-Path -LiteralPath $compiler)) {
    throw 'The bundled x64 LLVM-MinGW compiler was not found.'
}

$dist = Join-Path $PSScriptRoot 'dist'
New-Item -ItemType Directory -Force -Path $dist | Out-Null
$proxy = Join-Path $dist 'version.dll'
$smoke = Join-Path $dist 'VersionProxySmokeTest.exe'
$resolver = Join-Path $dist 'NBA2K27ProviderResolver.exe'

& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-shared' '-static' `
    '-Wl,--no-insert-timestamp' `
    (Join-Path $PSScriptRoot 'src\version_proxy.cpp') `
    (Join-Path $PSScriptRoot 'src\provider_resolver.cpp') `
    (Join-Path $PSScriptRoot 'src\version_proxy.def') `
    '-lbcrypt' `
    '-o' $proxy
if ($LASTEXITCODE -ne 0) { throw "Proxy compilation failed with exit code $LASTEXITCODE." }

& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-municode' '-static' `
    (Join-Path $PSScriptRoot 'src\smoke_test.cpp') `
    '-o' $smoke
if ($LASTEXITCODE -ne 0) { throw "Smoke-test compilation failed with exit code $LASTEXITCODE." }

& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-municode' '-static' `
    (Join-Path $PSScriptRoot 'src\resolver_cli.cpp') `
    (Join-Path $PSScriptRoot 'src\provider_resolver.cpp') `
    '-o' $resolver
if ($LASTEXITCODE -ne 0) { throw "Resolver compilation failed with exit code $LASTEXITCODE." }

& $smoke $proxy
if ($LASTEXITCODE -ne 0) { throw "Proxy smoke test failed with exit code $LASTEXITCODE." }

$readiness = Join-Path $dist 'NBA2K27AudioReadiness.exe'
& $compiler '--std=c++20' '-O2' '-Wall' '-Wextra' '-Werror' '-municode' '-static' (Join-Path $PSScriptRoot 'src\audio_readiness.cpp') '-o' $readiness
if ($LASTEXITCODE -ne 0) { throw 'Audio readiness helper compilation failed.' }
& $readiness '--self-test'
if ($LASTEXITCODE -ne 0) { throw 'Audio readiness helper self-test failed.' }

Get-Item -LiteralPath $proxy, $smoke, $resolver | Select-Object FullName, Length, LastWriteTime
