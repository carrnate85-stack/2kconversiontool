$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$build = Get-Content -LiteralPath (Join-Path $root 'dist/BUILD.json') -Raw | ConvertFrom-Json
$version = $build.version
$exe = Join-Path $root "dist/Character-Mod-Tool-Tauri-Preview-$version.exe"
$vendor = Join-Path $root 'artifacts/vendor/blender-5.1.2'
$blender = Join-Path $vendor 'blender-5.1.2-windows-x64.zip'
$source = Join-Path $vendor 'blender-5.1.2.tar.xz'
$officialHash = '345bedea7b0acf7cc9666423d8553f9129622aea34ded65c23e8cb70f83f14ff'
if ((Get-FileHash -LiteralPath $blender -Algorithm SHA256).Hash.ToLowerInvariant() -ne $officialHash) { throw 'Blender portable ZIP checksum mismatch.' }
if ((Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant() -ne 'aedfd030231979b629b18049d93b92fd2794952ca0cf39ebde54e84438bed05a') { throw 'Blender source checksum mismatch.' }
$prefix = "Character Mod Tool $version/"
Add-Type -AssemblyName System.IO.Compression
$reports = @()
foreach ($withBlender in @($false, $true)) {
    $label = if ($withBlender) { 'With-Blender-5.1.2' } else { 'Standard' }
    $destination = Join-Path $root "dist/Character-Mod-Tool-$version-$label.zip"
    $temporary = Join-Path $root ('dist/.portable-' + [Guid]::NewGuid().ToString('N') + '.preparing.zip')
    $stream = [IO.File]::Open($temporary, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
    $zip = [IO.Compression.ZipArchive]::new($stream, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($item in @(@($exe, 'Character-Mod-Tool.exe'), @((Join-Path $root 'release/README.txt'), 'README.txt'), @((Join-Path $root 'dist/BUILD.json'), 'BUILD.json'))) {
            $entry = $zip.CreateEntry($prefix + $item[1], [IO.Compression.CompressionLevel]::Optimal)
            $input = [IO.File]::OpenRead($item[0]); $output = $entry.Open()
            try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
        }
        $blenderFiles = 0
        if ($withBlender) {
            $vendorStream = [IO.File]::OpenRead($blender)
            $original = [IO.Compression.ZipArchive]::new($vendorStream, [IO.Compression.ZipArchiveMode]::Read)
            try {
                foreach ($member in $original.Entries) {
                    $relative = $member.FullName
                    if (!$relative.StartsWith('blender-5.1.2-windows-x64/', [StringComparison]::Ordinal)) { throw 'Unexpected Blender archive root.' }
                    $relative = $relative.Substring('blender-5.1.2-windows-x64/'.Length)
                    if (!$relative -or $relative.EndsWith('/')) { continue }
                    if ($relative -match '(^|/)\.\.(/|$)|\\|:') { throw 'Unsafe Blender member path.' }
                    $entry = $zip.CreateEntry($prefix + 'Blender/' + $relative, [IO.Compression.CompressionLevel]::Optimal)
                    $entry.LastWriteTime = $member.LastWriteTime
                    $input = $member.Open(); $output = $entry.Open()
                    try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
                    $blenderFiles++
                }
            } finally { $original.Dispose(); $vendorStream.Dispose() }
            $entry = $zip.CreateEntry($prefix + 'Sources/blender-5.1.2.tar.xz', [IO.Compression.CompressionLevel]::NoCompression)
            $input = [IO.File]::OpenRead($source); $output = $entry.Open()
            try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
        }
    } finally { $zip.Dispose(); $stream.Dispose() }
    if (Test-Path -LiteralPath $destination) { [IO.File]::Replace($temporary, $destination, [NullString]::Value) }
    else { [IO.File]::Move($temporary, $destination) }
    $reports += [ordered]@{file=[IO.Path]::GetFileName($destination); bytes=(Get-Item -LiteralPath $destination).Length;
        sha256=(Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant(); blenderFiles=$blenderFiles}
}
$report = [ordered]@{version=$version; builtUtc=[DateTime]::UtcNow.ToString('o');
    executableSha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant();
    blenderVersion='5.1.2'; blenderOfficialZipSha256=$officialHash;
    blenderSourceSha256=(Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash.ToLowerInvariant(); packages=$reports}
$report | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $root "artifacts/portable-release-$version.json") -Encoding utf8
$report | ConvertTo-Json -Depth 6
