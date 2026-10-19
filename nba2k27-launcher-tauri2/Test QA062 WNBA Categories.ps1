param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
try {
    & (Join-Path $PSScriptRoot 'Approve QA062 WNBA Categories.ps1') `
        -ExpectedDevExeSha256 'E77420948B8E29C4186F9FC5512508B673389E7AA9DCAD8F681E265DB52AD924' `
        -ExpectedGameDllSha256 'D39E5745655C8FE6EDACBA1C56BC12B232A2F101DC18A292D300EC8A2B27136A' `
        -CheckOnly:$CheckOnly
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [Windows.Forms.MessageBox]::Show($_.Exception.Message, 'WNBA category test did not start') | Out-Null
    exit 1
}
