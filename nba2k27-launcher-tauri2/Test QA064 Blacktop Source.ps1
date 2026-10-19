param([switch]$CheckOnly)
$ErrorActionPreference = 'Stop'
try {
    & (Join-Path $PSScriptRoot 'Approve QA064 Blacktop Source Test.ps1') `
        -ExpectedDevExeSha256 'BCB6FB163ACD7AB9F5041F91B85528ED87496A153F690AF16BA4FC433EFE2120' `
        -ExpectedGameDllSha256 '4164D1D8DB3161CA30688F41AD3ECDDDE152DB71C13D6458146F67741CDD9B71' `
        -CheckOnly:$CheckOnly
} catch {
    if ($CheckOnly) { Write-Output $_.Exception.Message; exit 1 }
    Add-Type -AssemblyName System.Windows.Forms
    [Windows.Forms.MessageBox]::Show($_.Exception.Message, 'Blacktop source test did not start') | Out-Null
    exit 1
}
