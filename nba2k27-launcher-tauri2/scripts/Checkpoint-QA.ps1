param([Parameter(Mandatory)][string]$Version,[Parameter(Mandatory)][string]$Name)
$ErrorActionPreference='Stop'
if($Version -notmatch '^\d+\.\d+\.\d+$' -or $Name -notmatch '^[A-Za-z0-9][A-Za-z0-9.-]{4,100}$'){throw 'Use a simple version and checkpoint name'}
$project=(Resolve-Path -LiteralPath (Split-Path $PSScriptRoot -Parent)).Path
$backupParent=[IO.Path]::GetFullPath((Join-Path $project 'Backups'))
$destination=[IO.Path]::GetFullPath((Join-Path $backupParent $Name))
if(-not $destination.StartsWith($backupParent+[IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)){throw 'Checkpoint must stay inside Backups'}
if(Test-Path -LiteralPath $destination){throw 'Checkpoint already exists; it will not be replaced'}
$portable=Join-Path $project ('Builds/'+$Version+'/Portable')
$info=Get-Content -LiteralPath (Join-Path $portable 'BUILD-INFO.json') -Raw|ConvertFrom-Json
$evidence=Get-Content -LiteralPath (Join-Path $project ('Builds/'+$Version+'/QA-VERIFICATION.json')) -Raw|ConvertFrom-Json
if($info.version -ne $Version -or $evidence.version -ne $Version -or $evidence.exeSha256 -ne $info.sha256 -or $evidence.mainInstalled -ne $false){throw 'Checkpoint metadata does not identify an isolated verified build'}
$sources=@()
foreach($directory in @('.cargo','assets','capabilities','frontend','native','permissions','scripts','src','test')){
    $sources+=@(Get-ChildItem -LiteralPath (Join-Path $project $directory) -Recurse -File | Where-Object {$_.FullName -notmatch '[\\/](node_modules|target|bin|obj)[\\/]'})
}
foreach($file in @('app-icon.ico','Build.ps1','build.rs','Cargo.lock','Cargo.toml','package.json','pnpm-lock.yaml','pnpm-workspace.yaml','QA-REPORT.md','README.md','tauri.conf.json','THIRD-PARTY-NOTICES.txt','tsconfig.json','vite.config.ts')){$sources+=Get-Item -LiteralPath (Join-Path $project $file)}
$entries=@()
foreach($source in $sources){$relative=[IO.Path]::GetRelativePath($project,$source.FullName);$entries+=@{source=$source.FullName;relative=$relative}}
foreach($relative in @('App/2k Launcher Rust Prototype.exe','BUILD-INFO.json','FEATURE-PARITY.json','README.md','QA-REPORT.md','THIRD-PARTY-NOTICES.txt')){$entries+=@{source=(Join-Path $portable $relative);relative=('Portable/'+$relative)}}
$entries+=@{source=(Join-Path $project ('Builds/'+$Version+'/QA-VERIFICATION.json'));relative='QA-VERIFICATION.json'}
$mp4Evidence=Join-Path $project ('Builds/'+$Version+'/SAVED-MP4-VERIFICATION.json')
if(Test-Path -LiteralPath $mp4Evidence){
    $mp4=Get-Content -LiteralPath $mp4Evidence -Raw|ConvertFrom-Json
    if($mp4.version -ne $Version -or $mp4.exeSha256 -ne $info.sha256 -or $mp4.pass -ne $true){throw 'Saved MP4 evidence does not identify this verified build'}
    $entries+=@{source=$mp4Evidence;relative='SAVED-MP4-VERIFICATION.json'}
}
$seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
foreach($entry in $entries){
    if(-not $seen.Add($entry.relative)){throw 'Duplicate checkpoint entry'}
    $file=Get-Item -LiteralPath $entry.source
    if($file.Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Linked checkpoint source refused'}
    $entry.hash=(Get-FileHash -LiteralPath $entry.source -Algorithm SHA256).Hash
}
$mainExe=($entries|Where-Object {$_.relative -eq 'Portable/App/2k Launcher Rust Prototype.exe'})
if($mainExe.hash -ne $info.sha256){throw 'Packaged executable hash differs'}
New-Item -ItemType Directory -Path $destination|Out-Null
$manifest=@()
foreach($entry in $entries){
    $target=Join-Path $destination $entry.relative
    New-Item -ItemType Directory -Path (Split-Path $target -Parent) -Force|Out-Null
    Copy-Item -LiteralPath $entry.source -Destination $target
    if((Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash -ne $entry.hash){throw 'Checkpoint copy hash differs'}
    $manifest+=@{path=$entry.relative;sha256=$entry.hash;bytes=(Get-Item -LiteralPath $target).Length}
}
$report=[ordered]@{version=$Version;createdUtc=[DateTime]::UtcNow.ToString('o');fileCount=$manifest.Count;verified=$true;mainInstalled=$false;userDataIncluded=$false;files=$manifest}
[IO.File]::WriteAllText((Join-Path $destination 'CHECKPOINT-MANIFEST.json'),($report|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
[pscustomobject]@{checkpoint=$destination;files=$manifest.Count;allHashesVerified=$true;userDataIncluded=$false}|ConvertTo-Json -Compress
