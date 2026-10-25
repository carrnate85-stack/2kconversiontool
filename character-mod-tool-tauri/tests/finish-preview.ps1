$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Split-Path $PSScriptRoot -Parent))
$version = (Get-Content -LiteralPath (Join-Path $root 'dist\BUILD.json') -Raw | ConvertFrom-Json).version
$exe = Join-Path $root ('dist\Character-Mod-Tool-Tauri-Preview-' + $version + '.exe')
if (!(Test-Path -LiteralPath $exe)) { throw 'Sharing executable missing.' }
$build = Get-Content -LiteralPath (Join-Path $root 'dist\BUILD.json') -Raw | ConvertFrom-Json
$engine = Join-Path $env:LOCALAPPDATA ('CharacterModToolTauri\Runtime\engine-' + $build.payloadSha256.Substring(0,16) + '\Backend')
if (!(Test-Path -LiteralPath $engine)) { throw 'Run a native test to prepare the current sharing runtime first.' }
$production = Join-Path (Split-Path $root -Parent) 'character-mod-tool-webview2\dist\Character-Mod-Tool-WebView2-0.1.31\App\Backend\blender_tools'
$verified = 0
foreach ($file in Get-ChildItem -LiteralPath $production -File -Recurse) {
    $relative = [IO.Path]::GetRelativePath($production, $file.FullName)
    if ($relative -match '(^|[\\/])__pycache__([\\/]|$)|\.pyc$') { continue }
    $bundled = Join-Path (Join-Path $engine 'blender_tools') $relative
    if ((Get-FileHash -LiteralPath $bundled).Hash -ne (Get-FileHash -LiteralPath $file.FullName).Hash) { throw "Blender tool changed: $relative" }
    $verified++
}
$desktop = [Environment]::GetFolderPath('Desktop')
$launcher = Join-Path $desktop 'Character Mod Tool Tauri Preview.lnk'
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($launcher)
$shortcut.TargetPath = $exe
$shortcut.WorkingDirectory = Split-Path $exe -Parent
$shortcut.IconLocation = "$exe,0"
$shortcut.Description = 'Character Mod Tool: Rust + Tauri 2 + TypeScript migration preview'
$shortcut.Save()

# Runtime retention belongs to the application's inventory and lease checks.
# A release helper must not bypass those checks for older or modified caches.
$artifacts = Join-Path $root 'artifacts'
foreach ($file in Get-ChildItem -LiteralPath $artifacts -File) {
    if ($file.Name -notmatch '^payload-[a-f0-9]{32}\.zip$') { continue }
    if ((Split-Path ([IO.Path]::GetFullPath($file.FullName)) -Parent) -ne $artifacts) { throw 'Unsafe build artifact path.' }
    Remove-Item -LiteralPath $file.FullName -Force
}
Write-Output "Verified $verified unchanged Blender tool files."
Write-Output "Launcher: $launcher"
Write-Output 'Prepared runtime retention is managed by the app; this helper did not delete runtime folders.'
