param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
try {
    & (Join-Path $PSScriptRoot 'Approve QA059 WNBA Source Test.ps1') `
        -ExpectedDevExeSha256 'C8EC75FF58FE9059D6D3E2C69A525FDD17508338E861C3FC00D8E12A3CDF76E6' `
        -ExpectedGameDllSha256 '4B03D0EAD15EB5CE562C434CD304017ADAD09DEFA96C1B4AAF30AA656830886F' `
        -CheckOnly:$CheckOnly
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [Windows.Forms.MessageBox]::Show($_.Exception.Message, 'WNBA source test did not start') | Out-Null
    exit 1
}
