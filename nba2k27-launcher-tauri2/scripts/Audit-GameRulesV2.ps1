param([string]$GameExe='C:\Program Files (x86)\Steam\steamapps\common\NBA 2K27\NBA2K27.exe')
$ErrorActionPreference='Stop'
$rulesImage=$GameExe
# Disk-only diagnostic. Getter patterns never grant process write authority.
. (Join-Path $PSScriptRoot 'Audit-GameBuild.ps1') -SelfTest | Out-Null
$rulesReference=Join-Path (Split-Path $PSScriptRoot -Parent) '..\nba2k27-mod-launcher\unlocks-helper\GameRuleData.cs'
$fields=[regex]::Matches((Get-Content -Raw -LiteralPath $rulesReference),'new RuleField\("[^"]+","([^"]+)",0x([0-9A-Fa-f]+),(1|4)\)')
if($fields.Count -ne 39){throw 'Expected the original 39-field WPF mapping'}
$patterns=[byte[][]]::new($fields.Count)
for($i=0;$i -lt $fields.Count;$i++){
  $offset=[Convert]::ToInt32($fields[$i].Groups[2].Value,16)
  $prefix=if($fields[$i].Groups[3].Value -eq '1'){[byte[]](0x0F,0xB6,0x80)}else{[byte[]](0x8B,0x80)}
  $patterns[$i]=[byte[]]($prefix+[BitConverter]::GetBytes($offset)+[byte[]](0xC3))
}
$stream=[IO.File]::Open($rulesImage,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
try {
  $sha=[Security.Cryptography.SHA256]::Create()
  try {$hash=[Convert]::ToHexString($sha.ComputeHash($stream))} finally {$sha.Dispose()}
  if($hash -ne '29A96471C55909FC2BCEA6C699C59238656A501C19B50C74ED64B0D842E576A1' -or $stream.Length -ne 870548760){throw 'This diagnostic is only for the pinned October 16 Version 2 image'}
  $found=[LauncherDiskPatternAudit]::Find($stream,1536,97458688,$patterns,1048576)
  $rows=for($i=0;$i -lt $fields.Count;$i++){
    $candidates=@(foreach($raw in $found[$i]){
      [void]$stream.Seek($raw-7,[IO.SeekOrigin]::Begin)
      $bytes=New-Object byte[] ($patterns[$i].Length+7)
      if($stream.Read($bytes,0,$bytes.Length) -ne $bytes.Length){throw 'Short getter read'}
      $rva=$raw-7-1536+4096
      if($bytes[0] -eq 0x48 -and $bytes[1] -eq 0x8B -and $bytes[2] -eq 0x05){
        $target=$rva+7+[BitConverter]::ToInt32($bytes,3)
        if($target -eq 0xAB5CE20){[ordered]@{rva=$rva;hex=[Convert]::ToHexString($bytes);target=$target}}
      }
    })
    $canonical=@($candidates|Where-Object {$_.rva -ge 0x25A0000 -and $_.rva -lt 0x25C0000}|Sort-Object rva)
    if($canonical.Count -eq 0){throw ('No independently anchored owner getter for '+$fields[$i].Groups[1].Value)}
    [ordered]@{id=$fields[$i].Groups[1].Value;offset=[Convert]::ToUInt32($fields[$i].Groups[2].Value,16);width=[int]$fields[$i].Groups[3].Value;rva=$canonical[0].rva;hex=$canonical[0].hex;ownerRva=$canonical[0].target;matchingOwnerGetters=$candidates.Count}
  }
  [ordered]@{schema='nba2k27-v2-game-rules-getters-v1';sha256=$hash;timestamp='6AC53765';imageSize='35BDE000';fields=@($rows);gameProcessAccess=$false;gameFilesChanged=$false;getterProofOnly=$true} | ConvertTo-Json -Depth 8
} finally {$stream.Dispose()}
