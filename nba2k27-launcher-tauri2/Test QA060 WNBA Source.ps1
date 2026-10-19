param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
try {
    & (Join-Path $PSScriptRoot 'Approve QA060 WNBA Source Test.ps1') `
        -ExpectedDevExeSha256 '46D50C305103726BFEA48D72A9D98DA77A473DCADCA00EC8571E451F60DE3806' `
        -ExpectedGameDllSha256 '4B03D0EAD15EB5CE562C434CD304017ADAD09DEFA96C1B4AAF30AA656830886F' `
        -CheckOnly:$CheckOnly
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [Windows.Forms.MessageBox]::Show($_.Exception.Message, 'WNBA source test did not start') | Out-Null
    exit 1
}
