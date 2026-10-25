param([string]$ExporterProject = 'D:\Projects\2k Exporter\nba2k27-exporter-tauri2')
$ErrorActionPreference = 'Stop'
$project = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
$env:CARGO_HOME = Join-Path $project '.toolchains\rust-exporter\cargo'
$env:RUSTUP_HOME = Join-Path $project '.toolchains\rust-exporter\rustup'
$vs = & 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe' -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$vc = Join-Path $vs 'VC\Auxiliary\Build\vcvars64.bat'
cmd /c "`"$vc`" >nul && set" | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') }
}
$env:PATH = "$env:CARGO_HOME\bin;$env:PATH"
$cargo = Join-Path $env:CARGO_HOME 'bin\cargo.exe'
foreach ($app in @((Join-Path $project 'shared\nba2k-assets'), $ExporterProject)) {
    Push-Location $app
    try {
        & $cargo test --offline
        if ($LASTEXITCODE) { throw "Rust tests failed: $app" }
    } finally { Pop-Location }
}
