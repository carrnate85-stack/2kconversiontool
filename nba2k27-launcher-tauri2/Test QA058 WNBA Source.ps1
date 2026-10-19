param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
try {
    & (Join-Path $PSScriptRoot 'Approve QA058 WNBA Source Test.ps1') `
        -ExpectedDevExeSha256 '310C0D60C900C27D1E3B5BAC057DE04560688AA209C0C2B74EED066F512DE3C7' `
        -ExpectedGameDllSha256 '4B03D0EAD15EB5CE562C434CD304017ADAD09DEFA96C1B4AAF30AA656830886F' `
        -CheckOnly:$CheckOnly
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [Windows.Forms.MessageBox]::Show($_.Exception.Message, 'WNBA source test did not start') | Out-Null
    exit 1
}
