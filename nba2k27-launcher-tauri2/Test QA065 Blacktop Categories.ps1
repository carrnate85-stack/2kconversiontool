# Exact final QA065 dev executable and native DLL pins.
param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
try {
    & (Join-Path $PSScriptRoot 'Approve QA065 Blacktop Categories.ps1') `
        -ExpectedDevExeSha256 'AA264F1AD5A630BBCDF059888E1FCE3F1FB2934E0722122B9EFD5107E8792740' `
        -ExpectedGameDllSha256 '376492792C2812947BB4200368C4737BDE5C59A9BF7A9375169E4FF6EF6EBEFB' `
        -ExpectedDevExeBytes 52292608 `
        -ExpectedGameDllBytes 1095680 `
        -CheckOnly:$CheckOnly
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [Windows.Forms.MessageBox]::Show($_.Exception.Message, 'WNBA category test did not start') | Out-Null
    exit 1
}
