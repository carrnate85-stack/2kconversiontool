param([switch]$SkipTests)
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
. (Join-Path $root 'build-tools\runtime-files.ps1')
. (Join-Path $root 'build-tools\build-inputs.ps1')
$project = Split-Path $root -Parent
$production = Join-Path $project 'character-mod-tool-webview2\dist\Character-Mod-Tool-WebView2-0.1.31\App\Backend'
if (!(Test-Path -LiteralPath (Join-Path $production 'python_bridge.py'))) { throw 'Verified production engine 0.1.31 is missing.' }
$roster = Join-Path (Split-Path $production -Parent) 'tools\roster.exe'
if ((Get-FileHash -LiteralPath $roster -Algorithm SHA256).Hash -ne '6F09FCEB91CAF946FF82FEEEE677256D50958BA9879B01478CEF4E6E3D2A3FCC') { throw 'Verified looyh roster CLI is missing or changed.' }
$env:CARGO_HOME = Join-Path $project '.toolchains\rust-exporter\cargo'
$env:RUSTUP_HOME = Join-Path $project '.toolchains\rust-exporter\rustup'
$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (!$vs) { throw 'MSVC C++ build tools are required for development.' }
$vc = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'
cmd /c "`"$vc`" >nul && set" | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
# Some standalone Build Tools installs do not discover an existing Windows SDK.
if (!$env:WindowsSdkDir -or !(Test-Path -LiteralPath (Join-Path $env:WindowsSdkDir "Lib/$env:WindowsSDKVersion/um/x64/OleAut32.lib"))) {
    $sdkRoot = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10'
    $sdk = Get-ChildItem -LiteralPath (Join-Path $sdkRoot 'Lib') -Directory | Where-Object {
        (Test-Path -LiteralPath (Join-Path $_.FullName 'um/x64/OleAut32.lib')) -and
        (Test-Path -LiteralPath (Join-Path $_.FullName 'ucrt/x64/libucrt.lib')) -and
        (Test-Path -LiteralPath (Join-Path $sdkRoot "Include/$($_.Name)/um/Windows.h"))
    } | Sort-Object { [version]$_.Name } -Descending | Select-Object -First 1
    if (!$sdk) { throw 'A complete Windows 10/11 x64 SDK is required for development.' }
    $env:WindowsSdkDir = $sdkRoot + '\'
    $env:WindowsSDKVersion = $sdk.Name + '\'
    $env:WindowsSDKLibVersion = $env:WindowsSDKVersion
    $env:LIB += ";$($sdk.FullName)/um/x64;$($sdk.FullName)/ucrt/x64"
    foreach ($part in @('ucrt', 'shared', 'um', 'winrt')) { $env:INCLUDE += ";$sdkRoot/Include/$($sdk.Name)/$part" }
    $env:PATH = "$sdkRoot/bin/$($sdk.Name)/x64;$env:PATH"
}
$env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
$cargo = Join-Path $env:CARGO_HOME 'bin\cargo.exe'
$payload = $null
$candidate = $null
$metadataCandidate = $null
Push-Location $root
try {
    & $cargo fmt
    if ($LASTEXITCODE) { throw 'Rust formatting failed.' }
    $inputs = Get-CharacterBuildInputs -Root $root -Production $production -Roster $roster
    & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'build-frontend.cjs')
    if ($LASTEXITCODE) { throw 'Tauri frontend build failed.' }
    Assert-BuildInputSnapshot $inputs (Get-CharacterBuildInputs -Root $root -Production $production -Roster $roster)
    $frontend = Get-BuildInputSnapshot -Trees @{ frontend = (Join-Path $root 'frontend') }
    & $cargo fmt --check
    if ($LASTEXITCODE) { throw 'Rust formatting failed.' }
    if (!$SkipTests) {
        & (Join-Path $root 'tests\runtime-policy.ps1')
        & (Join-Path $root 'tests\atomic-publication.ps1')
        & (Join-Path $root 'tests\build-inputs.ps1')
        & $cargo test --locked --manifest-path (Join-Path $project 'shared\nba2k-assets\Cargo.toml') --target-dir (Join-Path $root 'target')
        if ($LASTEXITCODE) { throw 'Shared native asset decoder tests failed.' }
        & $cargo test --locked; if ($LASTEXITCODE) { throw 'Rust tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\catalog-cache.cjs')
        if ($LASTEXITCODE) { throw 'Catalog cache regression tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\catalog-ui-cache.cjs')
        if ($LASTEXITCODE -ne 0) { throw 'Catalog UI cache tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\hair-target-ownership.cjs')
        if ($LASTEXITCODE -ne 0) { throw 'Hair target ownership tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\hair-catalog-ownership.cjs')
        if ($LASTEXITCODE -ne 0) { throw 'Hair catalog ownership tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\catalog-ui-latest.cjs')
        if ($LASTEXITCODE) { throw 'Latest catalog UI selection tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\worker-transfer.mjs')
        if ($LASTEXITCODE) { throw 'Worker result ownership tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\preview-packet.mjs')
        if ($LASTEXITCODE) { throw 'Binary preview packet tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\geometry-bridge.cjs')
        if ($LASTEXITCODE) { throw 'Binary geometry bridge tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\worker-entry.cjs')
        if ($LASTEXITCODE) { throw 'Worker entry regression tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\worker-latest.cjs')
        if ($LASTEXITCODE) { throw 'Latest preview queue regression tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\strand-preview.mjs')
        if ($LASTEXITCODE) { throw 'Strand shader regression tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\skin-response.mjs')
        if ($LASTEXITCODE) { throw 'Skin response regression tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\studio-shading.mjs')
        if ($LASTEXITCODE) { throw 'Studio shading regression tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\bundle-patches.cjs')
        if ($LASTEXITCODE) { throw 'Preview worker bundle patch tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\frontend-sync.cjs')
        if ($LASTEXITCODE) { throw 'Frontend renderer copies are out of date.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\renderer-transactions.cjs')
        if ($LASTEXITCODE) { throw 'Renderer texture transaction tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\staged-projection.mjs')
        if ($LASTEXITCODE) { throw 'Staged tangent projection regression tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\bridge-lifecycle.cjs')
        if ($LASTEXITCODE) { throw 'Bridge lifecycle regression tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\preview-scopes.cjs')
        if ($LASTEXITCODE) { throw 'Preview cancellation scope regression tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\preview-barriers.cjs')
        if ($LASTEXITCODE) { throw 'Preview cancellation barrier tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\source-freshness.cjs')
        if ($LASTEXITCODE) { throw 'Preview source freshness tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\player-preview-freshness.cjs')
        if ($LASTEXITCODE) { throw 'Player preview mutation freshness tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\preview-render-ownership.cjs')
        if ($LASTEXITCODE) { throw 'Renderer commit ownership tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\preview-resources.cjs')
        if ($LASTEXITCODE) { throw 'Preview resource ownership tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\test-watchdog-check.cjs')
        if ($LASTEXITCODE) { throw 'Lifecycle test watchdog verification failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\preview-fallback-diagnostics.cjs')
        if ($LASTEXITCODE) { throw 'Preview fallback diagnostic tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\staging-ownership.cjs')
        if ($LASTEXITCODE) { throw 'Staged player ownership tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\stage-player-ui.cjs')
        if ($LASTEXITCODE) { throw 'Direct player staging UI tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\player-staging.py')
        if ($LASTEXITCODE) { throw 'Direct player staging service tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\config-ui-ownership.cjs')
        if ($LASTEXITCODE) { throw 'Config UI ownership tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\player-edit-ownership.cjs')
        if ($LASTEXITCODE) { throw 'Player edit ownership tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\blender-ui-ownership.cjs')
        if ($LASTEXITCODE) { throw 'Blender UI ownership tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\blender-preparation.cjs')
        if ($LASTEXITCODE) { throw 'Blender preparation consistency tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\blender-snapshot-bridge.cjs')
        if ($LASTEXITCODE) { throw 'Blender snapshot bridge tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\preview-tab-status.cjs')
        if ($LASTEXITCODE) { throw 'Preview tab status ownership tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\reset-edit-drain.cjs')
        if ($LASTEXITCODE) { throw 'Reset edit draining tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\preview-draft-validation.cjs')
        if ($LASTEXITCODE) { throw 'Preview draft validation tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\preview-draft-commit.cjs')
        if ($LASTEXITCODE) { throw 'Preview draft commit tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\body-fit-reload.cjs')
        if ($LASTEXITCODE) { throw 'Body-fit reload synchronization tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\body-preview-editor.cjs')
        if ($LASTEXITCODE) { throw 'Body-fitted jersey preview tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\morph-decoder.cjs')
        if ($LASTEXITCODE) { throw 'Native jersey morph decoder tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\health-status-labels.cjs')
        if ($LASTEXITCODE) { throw 'Health diagnostic label tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\wpf-contracts.cjs')
        if ($LASTEXITCODE) { throw 'WPF public contract regression tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\preview-services.py')
        if ($LASTEXITCODE) { throw 'Staged preview resource regression tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\blender-services.py')
        if ($LASTEXITCODE) { throw 'Blender supervision regression tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\blender-snapshot-services.py')
        if ($LASTEXITCODE) { throw 'Blender snapshot service tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\player-publication.py')
        if ($LASTEXITCODE) { throw 'Atomic player publication tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\seams.py')
        if ($LASTEXITCODE) { throw 'Seam closer topology preservation tests failed.' }
        & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\seam-ui.cjs')
        if ($LASTEXITCODE) { throw 'Seam closer UI ownership tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -I -B (Join-Path $root 'tests\redistribution-dependencies.py') --self-test
        if ($LASTEXITCODE) { throw 'Redistribution dependency parser tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\runtime-paths.py')
        if ($LASTEXITCODE) { throw 'Managed runtime path tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\portable-blender.py')
        if ($LASTEXITCODE) { throw 'Portable Blender resolution tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\archive-comparison.py')
        if ($LASTEXITCODE) { throw 'Archive comparison regression tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\headband-fit-input.py')
        if ($LASTEXITCODE) { throw 'Headband fit input regression tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\accessory-distance.py')
        if ($LASTEXITCODE) { throw 'Accessory distance-reference regression tests failed.' }
        & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\accessory-install.py')
        if ($LASTEXITCODE) { throw 'Accessory installation and recovery tests failed.' }
    }
    Assert-BuildInputSnapshot $inputs (Get-CharacterBuildInputs -Root $root -Production $production -Roster $roster)
    Assert-BuildInputSnapshot $frontend (Get-BuildInputSnapshot -Trees @{ frontend = (Join-Path $root 'frontend') }) 'Generated frontend'
    & $cargo build --release --locked
    if ($LASTEXITCODE) { throw 'Rust release build failed.' }
    if (!$SkipTests) {
        $env:CMT_TEST_DDS_EXE = Join-Path $root 'target\release\character-mod-tool-tauri.exe'
        try {
            & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\geometry-decode.cjs')
            if ($LASTEXITCODE) { throw 'Native fitting decoder regression tests failed.' }
            & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\inherited-regressions.py')
            if ($LASTEXITCODE) { throw 'Inherited 2K25 and configuration regression tests failed.' }
        } finally { Remove-Item Env:CMT_TEST_DDS_EXE -ErrorAction SilentlyContinue }
    }
    Add-Type -AssemblyName System.IO.Compression
    $dist = Join-Path $root 'dist'
    [IO.Directory]::CreateDirectory($dist) | Out-Null
    $payload = Join-Path $root ('artifacts\payload-' + [Guid]::NewGuid().ToString('N') + '.zip')
    $inventory = [Collections.Generic.List[string]]::new()
    $omittedRuntimeFiles = 0
    $omittedRuntimeBytes = [long]0
    $stream = [IO.File]::Create($payload)
    $zip = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($file in (Get-ChildItem -LiteralPath $production -Recurse -File | Sort-Object FullName)) {
            $relative = $file.FullName.Substring($production.Length + 1).Replace('\', '/')
            if ($relative -eq 'tools/live_roster/roster.exe') { continue }
            if ($relative -match '(^|/)(__pycache__|\.pytest_cache|node_modules)(/|$)' -or $relative -match '\.py[co]$' -or $file.Name -eq 'node.exe') { continue }
            if (!(Test-PackagedRuntimeFile $relative)) {
                $omittedRuntimeFiles++
                $omittedRuntimeBytes += $file.Length
                continue
            }
            $entryName = 'Backend/' + $relative
            $entry = $zip.CreateEntry($entryName, [IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = [DateTimeOffset]::new(2000, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
            $input = [IO.File]::OpenRead($file.FullName)
            $output = $entry.Open()
            try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
            $inventory.Add($entryName)
        }
        foreach ($helper in (Get-ChildItem -LiteralPath (Join-Path $root 'engine') -File -Filter 'prototype_*.py' | Sort-Object Name)) {
            $entry = $zip.CreateEntry(('Backend/' + $helper.Name), [IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = [DateTimeOffset]::new(2000, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
            $input = [IO.File]::OpenRead($helper.FullName)
            $output = $entry.Open()
            try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
            $inventory.Add('Backend/' + $helper.Name)
        }
        $entry = $zip.CreateEntry('Backend/tools/live_roster/roster.exe', [IO.Compression.CompressionLevel]::Optimal)
        $entry.LastWriteTime = [DateTimeOffset]::new(2000, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
        $input = [IO.File]::OpenRead($roster)
        $output = $entry.Open()
        try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
        $inventory.Add('Backend/tools/live_roster/roster.exe')
        $entry = $zip.CreateEntry('Backend/electron/helpers/Nba2kVczDecoder.exe', [IO.Compression.CompressionLevel]::Optimal)
        $entry.LastWriteTime = [DateTimeOffset]::new(2000, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
        $input = [IO.File]::OpenRead((Join-Path $project 'character-mod-tool-webview2\dist\Character-Mod-Tool-WebView2-0.1.31\App\Reference\src\helpers\Nba2kVczDecoder.exe'))
        $output = $entry.Open()
        try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
        $inventory.Add('Backend/electron/helpers/Nba2kVczDecoder.exe')
    } finally { $zip.Dispose(); $stream.Dispose() }
    $binary = Join-Path $root 'target\release\character-mod-tool-tauri.exe'
    $version = (Get-Content -LiteralPath (Join-Path $root 'tauri.conf.json') -Raw | ConvertFrom-Json).version
    $sharing = Join-Path $dist "Character-Mod-Tool-Tauri-Preview-$version.exe"
    $candidate = Join-Path $dist ('.sharing-' + [Guid]::NewGuid().ToString('N') + '.preparing.exe')
    $metadataCandidate = Join-Path $dist ('.build-' + [Guid]::NewGuid().ToString('N') + '.preparing.json')
    Copy-Item -LiteralPath $binary -Destination $candidate
    $offset = (Get-Item -LiteralPath $candidate).Length
    $length = (Get-Item -LiteralPath $payload).Length
    $hash = (Get-FileHash -LiteralPath $payload -Algorithm SHA256).Hash.ToLowerInvariant()
    $input = [IO.File]::OpenRead($payload)
    $output = [IO.File]::Open($candidate, [IO.FileMode]::Append, [IO.FileAccess]::Write)
    try {
        $input.CopyTo($output)
        $output.Write([Text.Encoding]::ASCII.GetBytes('CMT-RUST-PAYLOAD'))
        $output.Write([BitConverter]::GetBytes([long]$offset))
        $output.Write([BitConverter]::GetBytes([long]$length))
        for ($i = 0; $i -lt $hash.Length; $i += 2) { $output.WriteByte([Convert]::ToByte($hash.Substring($i, 2), 16)) }
    } finally { $output.Dispose(); $input.Dispose() }
    $metadata = @{version=$version;host='Rust/Tauri 2';frontendLanguage='TypeScript';typeChecked=$true;nodeBackend=$false;dotnetHost=$false;
        bytes=(Get-Item -LiteralPath $candidate).Length;payloadSha256=$hash;engineVersion='0.1.31';files=$inventory.Count;
        runtimePolicyVersion=$script:RuntimePolicy.version;omittedRuntimeFiles=$omittedRuntimeFiles;omittedRuntimeBytes=$omittedRuntimeBytes;
        blenderFiles=@($inventory | Where-Object { $_.StartsWith('Backend/blender_tools/') }).Count;
        executableFiles=@($inventory | Where-Object { $_.EndsWith('.exe') });builtUtc=[DateTime]::UtcNow.ToString('o');
        buildInputsSha256=$inputs.Digest;buildInputFiles=$inputs.Files;frontendSha256=$frontend.Digest}
    $metadata | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $metadataCandidate -Encoding utf8
    & (Join-Path $production 'python_runtime\python.exe') -B (Join-Path $root 'tests\packaged-integrity.py') --exe $candidate --metadata $metadataCandidate
    if ($LASTEXITCODE) { throw 'Packaged payload and Blender tool integrity verification failed.' }
    if (!$SkipTests) {
        $env:CMT_TEST_RESOURCE_EXE = $candidate
        try {
            & 'C:\Users\carrn\.cache\codex-runtimes\codex-primary-runtime\dependencies\node\bin\node.exe' (Join-Path $root 'tests\preview-resource-rpc.cjs')
            if ($LASTEXITCODE) { throw 'Packaged native resource ownership tests failed.' }
        } finally { Remove-Item Env:CMT_TEST_RESOURCE_EXE -ErrorAction SilentlyContinue }
    }
    Assert-BuildInputSnapshot $inputs (Get-CharacterBuildInputs -Root $root -Production $production -Roster $roster)
    Assert-BuildInputSnapshot $frontend (Get-BuildInputSnapshot -Trees @{ frontend = (Join-Path $root 'frontend') }) 'Generated frontend'
    if (Test-Path -LiteralPath $sharing) { [IO.File]::Replace($candidate, $sharing, [NullString]::Value) }
    else { [IO.File]::Move($candidate, $sharing) }
    $metadataPath = Join-Path $dist 'BUILD.json'
    if (Test-Path -LiteralPath $metadataPath) { [IO.File]::Replace($metadataCandidate, $metadataPath, [NullString]::Value) }
    else { [IO.File]::Move($metadataCandidate, $metadataPath) }
    if (!([IO.Path]::GetFullPath($payload).StartsWith([IO.Path]::GetFullPath((Join-Path $root 'artifacts')) + '\', [StringComparison]::OrdinalIgnoreCase))) { throw 'Unsafe build cleanup path.' }
    Remove-Item -LiteralPath $payload
    Write-Output "Built $sharing"
    Write-Output "Size: $($metadata.bytes) bytes; Blender files preserved: $($metadata.blenderFiles)"
} finally {
    foreach ($temporary in @($payload, $candidate, $metadataCandidate)) {
        if (!$temporary -or !(Test-Path -LiteralPath $temporary)) { continue }
        $absolute = [IO.Path]::GetFullPath($temporary)
        if (!($absolute.StartsWith([IO.Path]::GetFullPath((Join-Path $root 'artifacts')) + '\', [StringComparison]::OrdinalIgnoreCase) -or
              $absolute.StartsWith([IO.Path]::GetFullPath((Join-Path $root 'dist')) + '\', [StringComparison]::OrdinalIgnoreCase))) { throw 'Unsafe build cleanup path.' }
        Remove-Item -LiteralPath $absolute
    }
    Pop-Location
}
