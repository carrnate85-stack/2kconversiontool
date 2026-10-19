param(
  [string]$ImageAuditJson,
  [string]$OutputPath,
  [uint32[]]$VtableRva=@(),
  [uint32[]]$ReferenceRva=@(),
  [string]$TypeRegex='PLAYNOW_BLACKTOP|PLAYER.*@CHARACTER|FRONTEND.*PLAYER|PLAYER.*MODEL|MODEL.*PLAYER|PLAYER.*MANAGER|INSTANCE.*@CHARACTER',
  [switch]$SelfTest
)
$ErrorActionPreference='Stop'
# Developer-only, disk-only class metadata audit. Results are candidates, not
# native read/write capabilities. No process handle or game DLL is opened.
if(-not ('LauncherDiskRttiAudit' -as [type])){
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
public sealed class LauncherDiskRttiRow {
  public string Name;
  public uint TypeRva, LocatorRva, VtableRva, ObjectOffset;
  public uint[] FirstMethods;
}
public sealed class LauncherDiskRipReference {
  public uint InstructionRva, TargetRva;
  public string Operation;
}
public static class LauncherDiskRttiAudit {
  public static List<LauncherDiskRipReference> References(byte[] code,uint codeRva,uint[] targets) {
    if(code==null || code.Length>128*1024*1024 || (ulong)codeRva+(uint)code.Length>uint.MaxValue)
      throw new ArgumentException("Invalid bounded code range");
    var wanted=new HashSet<uint>(targets);
    var rows=new List<LauncherDiskRipReference>();
    for(int at=0;at<code.Length;at++){
      bool rex=code[at]==0x48 || code[at]==0x4c;
      if(!rex && at>0 && (code[at-1]==0x48 || code[at-1]==0x4c))continue;
      int op=at+(rex?1:0);
      if(op+1>=code.Length)continue;
      int modrm=op+1,immediate=0;
      string operation;
      switch(code[op]){
        case 0x8d:operation="lea";break;
        case 0x8b:operation="mov-load";break;
        case 0x89:operation="mov-store";break;
        case 0x63:operation="movsxd-load";break;
        case 0x0f:
          if(op+2>=code.Length || (code[op+1]!=0xb6 && code[op+1]!=0xb7))continue;
          modrm=op+2;operation="movzx-load";break;
        case 0xc6:operation="mov-store-immediate";immediate=1;break;
        case 0xc7:operation="mov-store-immediate";immediate=4;break;
        case 0x80:case 0x83:operation="memory-immediate";immediate=1;break;
        case 0x81:operation="memory-immediate";immediate=4;break;
        case 0xf6:
          if((code[modrm]&0x38)!=0)continue;
          operation="test-immediate";immediate=1;break;
        default:continue;
      }
      int length=modrm-at+1+4+immediate;
      if(at+length>code.Length || (code[modrm]&0xc7)!=0x05)continue;
      long target=(long)codeRva+at+length+BitConverter.ToInt32(code,modrm+1);
      if(target<0 || target>uint.MaxValue || !wanted.Contains((uint)target))continue;
      if(rows.Count>=4096)throw new InvalidOperationException("Ambiguous reference count exceeds audit cap");
      rows.Add(new LauncherDiskRipReference {
        InstructionRva=codeRva+(uint)at,TargetRva=(uint)target,
        Operation=operation
      });
    }
    return rows;
  }
  public static List<LauncherDiskRttiRow> Find(
      byte[] data,uint sectionRva,ulong imageBase,
      Dictionary<uint,string> types,uint codeRva,uint codeBytes) {
    if(data==null || data.Length>128*1024*1024 || (ulong)sectionRva+(uint)data.Length>uint.MaxValue ||
       (ulong)codeRva+codeBytes>uint.MaxValue || codeBytes==0)
      throw new ArgumentException("Invalid bounded RTTI sections");
    var locators=new Dictionary<ulong,LauncherDiskRttiRow>();
    for(int at=0;at<=data.Length-24;at+=4){
      if(data[at]!=1 || data[at+1]!=0 || data[at+2]!=0 || data[at+3]!=0)continue;
      uint self=BitConverter.ToUInt32(data,at+20);
      if(self!=sectionRva+(uint)at)continue;
      uint type=BitConverter.ToUInt32(data,at+12);
      string name;
      if(!types.TryGetValue(type,out name))continue;
      if(BitConverter.ToUInt32(data,at+8)!=0)continue;
      locators.Add(imageBase+self,new LauncherDiskRttiRow {
        Name=name,TypeRva=type,LocatorRva=self,ObjectOffset=BitConverter.ToUInt32(data,at+4)
      });
    }
    var rows=new List<LauncherDiskRttiRow>();
    for(int at=0;at<=data.Length-16;at+=8){
      LauncherDiskRttiRow locator;
      if(!locators.TryGetValue(BitConverter.ToUInt64(data,at),out locator))continue;
      var methods=new List<uint>();
      for(int slot=0;slot<8 && at+16+slot*8<=data.Length;slot++){
        ulong target=BitConverter.ToUInt64(data,at+8+slot*8);
        if(target<imageBase || target-imageBase<codeRva ||
           target-imageBase>=(ulong)codeRva+codeBytes)break;
        methods.Add((uint)(target-imageBase));
      }
      if(methods.Count==0)continue;
      rows.Add(new LauncherDiskRttiRow {
        Name=locator.Name,TypeRva=locator.TypeRva,LocatorRva=locator.LocatorRva,
        ObjectOffset=locator.ObjectOffset,VtableRva=sectionRva+(uint)at+8,FirstMethods=methods.ToArray()
      });
    }
    return rows;
  }
}
'@
}
if($SelfTest){
  $taskData=New-Object byte[] 256
  $taskTypes=[Collections.Generic.Dictionary[uint32,string]]::new()
  $taskTypes.Add(0x4010,'.?AVTEST@@')
  function Put-RttiU32([int]$At,[uint32]$Value){[BitConverter]::GetBytes($Value).CopyTo($taskData,$At)}
  Put-RttiU32 32 1
  Put-RttiU32 44 0x4010
  Put-RttiU32 52 0x2020
  [BitConverter]::GetBytes([uint64]0x140002020).CopyTo($taskData,128)
  [BitConverter]::GetBytes([uint64]0x140001100).CopyTo($taskData,136)
  $taskFound=[LauncherDiskRttiAudit]::Find($taskData,0x2000,0x140000000,$taskTypes,0x1000,0x1000)
  if($taskFound.Count -ne 1 -or $taskFound[0].VtableRva -ne 0x2088 -or $taskFound[0].Name -ne '.?AVTEST@@'){throw 'RTTI self-reference or table association failed'}
  Put-RttiU32 52 0x2024
  if([LauncherDiskRttiAudit]::Find($taskData,0x2000,0x140000000,$taskTypes,0x1000,0x1000).Count){throw 'Forged RTTI self address accepted'}
  Put-RttiU32 52 0x2020
  Put-RttiU32 44 0x4020
  if([LauncherDiskRttiAudit]::Find($taskData,0x2000,0x140000000,$taskTypes,0x1000,0x1000).Count){throw 'Foreign type accepted'}
  Put-RttiU32 44 0x4010
  [BitConverter]::GetBytes([uint64]0x140003000).CopyTo($taskData,136)
  if([LauncherDiskRttiAudit]::Find($taskData,0x2000,0x140000000,$taskTypes,0x1000,0x1000).Count){throw 'Non-code vtable target accepted'}
  [BitConverter]::GetBytes([uint64]0x140001100).CopyTo($taskData,136)
  Put-RttiU32 36 24
  $taskFound=[LauncherDiskRttiAudit]::Find($taskData,0x2000,0x140000000,$taskTypes,0x1000,0x1000)
  if($taskFound[0].ObjectOffset -ne 24){throw 'Secondary vtable offset was hidden'}
  Put-RttiU32 40 1
  if([LauncherDiskRttiAudit]::Find($taskData,0x2000,0x140000000,$taskTypes,0x1000,0x1000).Count){throw 'Construction-displacement RTTI accepted'}
  $taskCode=New-Object byte[] 32
  ([byte[]](0x48,0x8D,0x0D)).CopyTo($taskCode,0)
  [BitConverter]::GetBytes([int32](0x4000-0x1000-7)).CopyTo($taskCode,3)
  ([byte[]](0x4C,0x8B,0x05)).CopyTo($taskCode,8)
  [BitConverter]::GetBytes([int32](0x500-0x1000-8-7)).CopyTo($taskCode,11)
  $taskRefs=[LauncherDiskRttiAudit]::References($taskCode,0x1000,[uint32[]](0x4000,0x500))
  if($taskRefs.Count -ne 2 -or $taskRefs[0].InstructionRva -ne 0x1000 -or $taskRefs[1].TargetRva -ne 0x500){throw 'Signed RIP-relative reference decoding failed'}
  $taskCode[2]=0x0C
  if([LauncherDiskRttiAudit]::References($taskCode,0x1000,[uint32[]](0x4000)).Count){throw 'Non-RIP addressing accepted'}
  ([byte[]](0x8B,0x05)).CopyTo($taskCode,16)
  [BitConverter]::GetBytes([int32](0x6000-0x1000-16-6)).CopyTo($taskCode,18)
  $taskRefs=[LauncherDiskRttiAudit]::References($taskCode,0x1000,[uint32[]](0x6000))
  if($taskRefs.Count -ne 1 -or $taskRefs[0].InstructionRva -ne 0x1010 -or $taskRefs[0].Operation -ne 'mov-load'){throw 'Dword RIP load decoding failed'}
  $taskCode[16]=0x89
  $taskRefs=[LauncherDiskRttiAudit]::References($taskCode,0x1000,[uint32[]](0x6000))
  if($taskRefs.Count -ne 1 -or $taskRefs[0].Operation -ne 'mov-store'){throw 'RIP store classification failed'}
  $taskCode[9]=0x63
  if([LauncherDiskRttiAudit]::References($taskCode,0x1000,[uint32[]](0x500))[0].Operation -ne 'movsxd-load'){throw 'Signed mode-index load decoding failed'}
  $taskLast=[byte[]](0x8B,0x05,0,0,0,0)
  [BitConverter]::GetBytes([int32](0x3000-0x1000-6)).CopyTo($taskLast,2)
  if([LauncherDiskRttiAudit]::References($taskLast,0x1000,[uint32[]](0x3000)).Count -ne 1){throw 'Final bounded dword load was missed'}
  $taskByte=[byte[]](0x0F,0xB6,0x05,0,0,0,0)
  [BitConverter]::GetBytes([int32](0x3000-0x1000-7)).CopyTo($taskByte,3)
  if([LauncherDiskRttiAudit]::References($taskByte,0x1000,[uint32[]](0x3000))[0].Operation -ne 'movzx-load'){throw 'Byte flag load decoding failed'}
  $taskByte=[byte[]](0xC6,0x05,0,0,0,0,1)
  [BitConverter]::GetBytes([int32](0x3000-0x1000-7)).CopyTo($taskByte,2)
  if([LauncherDiskRttiAudit]::References($taskByte,0x1000,[uint32[]](0x3000))[0].Operation -ne 'mov-store-immediate'){throw 'Immediate store next-instruction base was wrong'}
  [pscustomobject]@{pass=$true;checks=14;gameProcessAccess=$false;diskGameAccess=$false}|ConvertTo-Json -Compress
  return
}
if(-not $ImageAuditJson){throw 'Pass an existing disk-image audit, or use -SelfTest'}
$taskAudit=Get-Content -LiteralPath $ImageAuditJson -Raw|ConvertFrom-Json
if($taskAudit.schema -ne 'nba2k27-disk-anchor-audit-v1' -or $taskAudit.gameProcessAccess -ne $false){throw 'Unsupported disk audit'}
$taskGame=(Resolve-Path -LiteralPath $taskAudit.path).Path
if([IO.Path]::GetFileName($taskGame) -ine 'NBA2K27.exe' -or $taskAudit.sha256 -notmatch '^[A-F0-9]{64}$'){throw 'Invalid image identity'}
$taskCode=@($taskAudit.sections|Where-Object {($_.flags -band 0x20000000) -and -not ($_.flags -band 0x80000000L)})
$taskRdata=@($taskAudit.sections|Where-Object {$_.name -eq '.rdata'})
$taskData=@($taskAudit.sections|Where-Object {$_.name -eq '.data'})
if($taskCode.Count -ne 1 -or $taskRdata.Count -ne 1 -or $taskData.Count -ne 1){throw 'Unsupported code/type-data section layout'}
$taskStream=[IO.File]::Open($taskGame,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
try {
  $taskHasher=[Security.Cryptography.SHA256]::Create()
  try {$taskSha=[BitConverter]::ToString($taskHasher.ComputeHash($taskStream)).Replace('-','')} finally {$taskHasher.Dispose()}
  if($taskSha -ne $taskAudit.sha256 -or $taskStream.Length -ne $taskAudit.bytes){throw 'Image no longer matches the preceding disk audit'}
  $taskReader=[IO.BinaryReader]::new($taskStream,[Text.Encoding]::UTF8,$true)
  $taskStream.Position=0
  $taskHeader=$taskReader.ReadBytes(4096)
  $taskPe=[BitConverter]::ToUInt32($taskHeader,60)
  if($taskPe -lt 64 -or $taskPe -gt 3800 -or [Text.Encoding]::ASCII.GetString($taskHeader,$taskPe,4) -ne "PE$([char]0)$([char]0)"){throw 'Invalid image header'}
  $taskImageBase=[BitConverter]::ToUInt64($taskHeader,$taskPe+48)
  function Read-RttiSection($TaskSection){
    if($TaskSection.rawBytes -lt 24 -or $TaskSection.rawBytes -gt 134217728 -or $TaskSection.rawOffset -lt 0 -or $TaskSection.rawOffset -gt $taskStream.Length-$TaskSection.rawBytes){throw 'Section escapes bounded file'}
    $taskStream.Position=$TaskSection.rawOffset
    $taskBytes=$taskReader.ReadBytes($TaskSection.rawBytes)
    if($taskBytes.Length -ne $TaskSection.rawBytes){throw 'Truncated section'}
    return ,$taskBytes
  }
  $taskTypeBytes=Read-RttiSection $taskData[0]
  $taskTypeText=[Text.Encoding]::ASCII.GetString($taskTypeBytes)
  $taskTypes=[Collections.Generic.Dictionary[uint32,string]]::new()
  foreach($taskMatch in [regex]::Matches($taskTypeText,'\.\?AV[^\x00]{1,220}')){
    if($taskMatch.Index -ge 16 -and ($VtableRva.Count -gt 0 -or $taskMatch.Value -match $TypeRegex) -and $taskMatch.Value -notmatch 'VCFEATURECODE_'){
      $taskTypeRva=[uint32]($taskData[0].rva+$taskMatch.Index-16)
      $taskTypes.Add($taskTypeRva,$taskMatch.Value)
    }
  }
  $taskRows=[LauncherDiskRttiAudit]::Find((Read-RttiSection $taskRdata[0]),$taskRdata[0].rva,$taskImageBase,$taskTypes,$taskCode[0].rva,$taskCode[0].rawBytes)
  if($VtableRva.Count -gt 0){$taskRows=@($taskRows|Where-Object {$VtableRva -contains $_.VtableRva})}
  $taskReferences=@()
  if($ReferenceRva.Count -gt 0){$taskReferences=[LauncherDiskRttiAudit]::References((Read-RttiSection $taskCode[0]),$taskCode[0].rva,$ReferenceRva)}
  $taskReport=[ordered]@{
    schema='nba2k27-disk-rtti-audit-v1';utc=[DateTime]::UtcNow.ToString('o');imageSha256=$taskSha
    timestamp=$taskAudit.timestamp;sizeOfImage=$taskAudit.sizeOfImage;gameProcessAccess=$false;gameFilesChanged=$false;candidateOnly=$true
    typeDescriptors=$taskTypes.Count;rows=@($taskRows|ForEach-Object {@{name=$_.Name;typeRva=('{0:X8}' -f $_.TypeRva);locatorRva=('{0:X8}' -f $_.LocatorRva);vtableRva=('{0:X8}' -f $_.VtableRva);objectOffset=$_.ObjectOffset;firstMethods=@($_.FirstMethods|ForEach-Object {'{0:X8}' -f $_})}})
    references=@($taskReferences|ForEach-Object {@{instructionRva=('{0:X8}' -f $_.InstructionRva);targetRva=('{0:X8}' -f $_.TargetRva);operation=$_.Operation;disassemblyRequired=$true}})
  }
} finally {$taskStream.Dispose()}
if($OutputPath){
  $taskArtifacts=[IO.Path]::GetFullPath((Join-Path (Split-Path $PSScriptRoot -Parent) 'artifacts'))+[IO.Path]::DirectorySeparatorChar
  $taskOutput=[IO.Path]::GetFullPath($OutputPath)
  if(-not $taskOutput.StartsWith($taskArtifacts,[StringComparison]::OrdinalIgnoreCase)){throw 'Output must stay inside project artifacts'}
  $taskOutputStream=[IO.File]::Open($taskOutput,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
  try {
    $taskJson=$taskReport|ConvertTo-Json -Depth 10
    $taskBytes=[Text.UTF8Encoding]::new($false).GetBytes($taskJson)
    $taskOutputStream.Write($taskBytes,0,$taskBytes.Length)
  } finally {$taskOutputStream.Dispose()}
}
[pscustomobject]@{pass=$true;typeDescriptors=$taskTypes.Count;vtableCandidates=$taskRows.Count;output=$OutputPath;gameProcessAccess=$false}|ConvertTo-Json -Compress
