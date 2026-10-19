param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference='Stop'
if($Version -notmatch '^\d+\.\d+\.\d+$'){throw 'Use an exact three-part QA version'}
$project=Split-Path $PSScriptRoot -Parent
$portable=(Resolve-Path -LiteralPath (Join-Path $project "Builds/$Version/Portable")).Path
$build=Get-Content -LiteralPath (Join-Path $portable 'BUILD-INFO.json') -Raw | ConvertFrom-Json
if($build.version -ne $Version -or -not $build.isolatedUiVerified -or $build.mainInstalled -or $build.parityComplete){throw 'Not an isolated verified QA checkpoint'}
$entries=@('App/2k Launcher Rust Prototype.exe','BUILD-INFO.json','FEATURE-PARITY.json','QA-REPORT.md','README.md','THIRD-PARTY-NOTICES.txt')
$executable=Join-Path $portable $entries[0]
if((Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash -ne $build.sha256 -or (Get-Item -LiteralPath $executable).Length -ne $build.bytes){throw 'EXE differs from verified build information'}
foreach($entry in $entries){if(-not (Test-Path -LiteralPath (Join-Path $portable $entry) -PathType Leaf)){throw "Missing share file: $entry"}}
# An explicit allowlist: never recurse through a used Portable folder. User
# Data, Library, WebView profile, cache, logs and browser-session tokens cannot
# enter this package. CreateNew refuses to overwrite an existing share ZIP.
$destination=Join-Path $project "Builds/$Version/2k-Launcher-Rust-Prototype-$Version.zip"
$stream=[IO.File]::Open($destination,[IO.FileMode]::CreateNew,[IO.FileAccess]::ReadWrite,[IO.FileShare]::None)
try {
    $zip=[IO.Compression.ZipArchive]::new($stream,[IO.Compression.ZipArchiveMode]::Create,$true)
    try {
        foreach($entry in $entries){
            $row=$zip.CreateEntry(('Portable/'+$entry),[IO.Compression.CompressionLevel]::Optimal)
            $output=$row.Open()
            $input=[IO.File]::OpenRead((Join-Path $portable $entry))
            try {$input.CopyTo($output)} finally {$input.Dispose();$output.Dispose()}
        }
    } finally {$zip.Dispose()}
} finally {$stream.Dispose()}
$zip=[IO.Compression.ZipFile]::OpenRead($destination)
try {
    if($zip.Entries.Count -ne $entries.Count){throw 'Unexpected ZIP entry count'}
    foreach($entry in $entries){
        $row=$zip.GetEntry('Portable/'+$entry)
        if(-not $row){throw "Missing ZIP entry: $entry"}
        $input=$row.Open()
        try {$hash=[Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($input))} finally {$input.Dispose()}
        if($hash -ne (Get-FileHash -LiteralPath (Join-Path $portable $entry) -Algorithm SHA256).Hash){throw "ZIP content mismatch: $entry"}
    }
} finally {$zip.Dispose()}
[pscustomobject]@{path=$destination;entries=$entries.Count;bytes=(Get-Item -LiteralPath $destination).Length;sha256=(Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash;userDataIncluded=$false} | ConvertTo-Json
