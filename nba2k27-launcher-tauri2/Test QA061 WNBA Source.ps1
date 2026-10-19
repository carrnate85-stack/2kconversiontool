param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
try {
    & (Join-Path $PSScriptRoot 'Approve QA061 WNBA Source Test.ps1') `
        -ExpectedDevExeSha256 '7219D0729FF4BFB290A0A37655C6831D550A2CC909324D818CB891BF008B9D59' `
        -ExpectedGameDllSha256 'D39E5745655C8FE6EDACBA1C56BC12B232A2F101DC18A292D300EC8A2B27136A' `
        -CheckOnly:$CheckOnly
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [Windows.Forms.MessageBox]::Show($_.Exception.Message, 'WNBA source test did not start') | Out-Null
    exit 1
}
