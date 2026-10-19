param([string]$GameExe,[string]$OutputPath,[switch]$SelfTest)
$ErrorActionPreference='Stop'
# Disk-only developer diagnostic. Never opens a process, loads the game DLL,
# patches bytes, relaxes a build guard or treats a pattern hit as write authority.
if(-not ('LauncherDiskPatternAudit' -as [type])){
Add-Type -TypeDefinition @'
using System;
using System.IO;
using System.Collections.Generic;
public static class LauncherDiskPatternAudit {
  public static List<long>[] Find(Stream input,long start,long length,byte[][] patterns,int blockSize) {
    if(!input.CanSeek || !input.CanRead || start<0 || length<0 || start>input.Length-length || blockSize<32)
      throw new ArgumentException("Invalid bounded disk range");
    int overlap=0;
    var result=new List<long>[patterns.Length];
    for(int p=0;p<patterns.Length;p++){
      if(patterns[p]==null || patterns[p].Length<4 || patterns[p].Length>256)
        throw new ArgumentException("Invalid diagnostic pattern");
      overlap=Math.Max(overlap,patterns[p].Length-1);
      result[p]=new List<long>();
    }
    var buffer=new byte[blockSize+overlap];
    long consumed=0;
    int retained=0;
    input.Position=start;
    while(consumed<length){
      int count=(int)Math.Min(blockSize,length-consumed);
      int done=0;
      while(done<count){
        int n=input.Read(buffer,retained+done,count-done);
        if(n==0)throw new EndOfStreamException("Game image changed or is truncated");
        done+=n;
      }
      int total=retained+count;
      long basis=start+consumed-retained;
      for(int p=0;p<patterns.Length;p++){
        if(result[p].Count>=65)continue;
        byte[] pattern=patterns[p];
        int at=0;
        while(at<=total-pattern.Length){
          at=Array.IndexOf(buffer,pattern[0],at,total-pattern.Length-at+1);
          if(at<0)break;
          bool equal=true;
          for(int j=1;j<pattern.Length;j++)if(buffer[at+j]!=pattern[j]){equal=false;break;}
          if(equal){
            long found=basis+at;
            // Do not count the same hit again in the retained chunk suffix.
            if(result[p].Count==0 || found>result[p][result[p].Count-1]){
              result[p].Add(found);
              if(result[p].Count>=65)break;
            }
          }
          at++;
        }
      }
      consumed+=count;
      retained=Math.Min(overlap,total);
      Buffer.BlockCopy(buffer,total-retained,buffer,0,retained);
    }
    return result;
  }
}
'@
}
if($SelfTest){
  $taskPattern=[byte[]](1,2,3,4,5,6,7)
  $taskData=New-Object byte[] 159
  [Array]::Copy($taskPattern,0,$taskData,29,$taskPattern.Length)
  [Array]::Copy($taskPattern,0,$taskData,64,$taskPattern.Length)
  [Array]::Copy($taskPattern,0,$taskData,151,$taskPattern.Length)
  $taskInput=[IO.MemoryStream]::new($taskData,$false)
  $taskFixturePatterns=[byte[][]]::new(1)
  $taskFixturePatterns[0]=$taskPattern
  try {
    $taskFound=[LauncherDiskPatternAudit]::Find($taskInput,0,$taskData.Length,$taskFixturePatterns,32)
    if(($taskFound[0] -join ',') -ne '29,64,151'){throw 'Chunk-boundary scan failed'}
    $taskFound=[LauncherDiskPatternAudit]::Find($taskInput,36,115,$taskFixturePatterns,32)
    if(($taskFound[0] -join ',') -ne '64'){throw 'Bounded scan escaped its range'}
    $taskRejected=$false
    try { [LauncherDiskPatternAudit]::Find($taskInput,0,160,$taskFixturePatterns,32)|Out-Null } catch {$taskRejected=$true}
    if(-not $taskRejected){throw 'Out-of-file scan accepted'}
    $taskMany=[IO.MemoryStream]::new(([byte[]](1,2,3,4)*100),$false)
    try {
      $taskFixturePatterns[0]=[byte[]](1,2,3,4)
      $taskFound=[LauncherDiskPatternAudit]::Find($taskMany,0,$taskMany.Length,$taskFixturePatterns,32)
      if($taskFound[0].Count -ne 65){throw 'Ambiguous hit cap failed'}
    } finally {$taskMany.Dispose()}
  } finally {$taskInput.Dispose()}
  [pscustomobject]@{pass=$true;checks=4;gameProcessAccess=$false;diskGameAccess=$false}|ConvertTo-Json -Compress
  return
}
if(-not $GameExe){throw 'Pass the exact installed NBA2K27.exe path, or use -SelfTest'}
$taskGame=(Resolve-Path -LiteralPath $GameExe).Path
if([IO.Path]::GetFileName($taskGame) -ine 'NBA2K27.exe'){throw 'Expected NBA2K27.exe'}
if((Get-Item -LiteralPath $taskGame).Attributes -band [IO.FileAttributes]::ReparsePoint){throw 'Linked executable refused'}
$taskDefinitions=@(
  @{name='selection-clothing-refresh';rva=0x3ACB20;scope='code';hex='83 FA 1E 77 0E 89 D0 48 69 C0 B0 07 00 00 C6 44 01 30 01 C3'},
  @{name='match-description-virtual-clone';rva=0x729250;scope='code';hex='55 41 56 56 57 53 48 83 EC 50 48 8D 6C 24 50 48 C7 45 F8 FE FF FF FF 48 89 D7 48 89 CE 48 8B 01 FF 90 D8 00 00 00 48 85 C0'},
  @{name='match-outfit-array-copy';rva=0x7AAB70;scope='code';hex='56 57 53 48 83 EC 60 44 89 C3 48 89 D7 48 89 CE 44 89 44 24 4C 83 79 08 00'},
  @{name='arena-floor-field';rva=0x2EE8EDC;scope='code';hex='0F B7 88 92 07 00 00 39 CE 49 0F 45 C5'},
  @{name='ball-live-record-field';rva=0x4138B79;scope='code';hex='41 8B 8E 4C 03 00 00'},
  @{name='ball-record-step';rva=0x21BFED8;scope='code';hex='48 81 C7 9C 00 00 00'},
  @{name='ball-record-type';rva=0x21BE198;scope='code';hex='81 FF C5 FE 1B DC'},
  @{name='ball-record-label';rva=0x21BE1A0;scope='code';hex='C7 44 24 48 BE 91 56 CB'},
  @{name='ball-render-size';rva=0x31436D0;scope='code';hex='48 89 D0 C7 02 C0 64 99 07 C3'},
  @{name='quickplay-mode-name';rva=0x77F9368;scope='data';hex='4E 42 41 5F 4D 61 69 6E 4D 65 6E 75 5F 50 6C 61 79 4E 6F 77 00'}
)
foreach($taskDefinition in $taskDefinitions){
  $taskDefinition.bytes=[byte[]]@($taskDefinition.hex.Split(' ')|ForEach-Object {[Convert]::ToByte($_,16)})
  $taskDefinition.hits=[Collections.Generic.List[object]]::new()
}
$taskBefore=Get-Item -LiteralPath $taskGame
$taskStream=[IO.File]::Open($taskGame,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
try {
  $taskReader=[IO.BinaryReader]::new($taskStream,[Text.Encoding]::UTF8,$true)
  $taskHeader=$taskReader.ReadBytes(4096)
  if($taskHeader.Length -ne 4096 -or [Text.Encoding]::ASCII.GetString($taskHeader,0,2) -ne 'MZ'){throw 'Invalid executable header'}
  $taskPe=[BitConverter]::ToUInt32($taskHeader,60)
  if($taskPe -lt 64 -or $taskPe -gt 3800 -or [Text.Encoding]::ASCII.GetString($taskHeader,$taskPe,4) -ne "PE$([char]0)$([char]0)"){throw 'Invalid PE header'}
  $taskMachine=[BitConverter]::ToUInt16($taskHeader,$taskPe+4)
  if($taskMachine -ne 0x8664 -or [BitConverter]::ToUInt16($taskHeader,$taskPe+24) -ne 0x20B){throw 'Expected PE32+ x64'}
  $taskStamp=[BitConverter]::ToUInt32($taskHeader,$taskPe+8)
  $taskSize=[BitConverter]::ToUInt32($taskHeader,$taskPe+80)
  $taskBase=[BitConverter]::ToUInt64($taskHeader,$taskPe+48)
  $taskCount=[BitConverter]::ToUInt16($taskHeader,$taskPe+6)
  $taskOptional=[BitConverter]::ToUInt16($taskHeader,$taskPe+20)
  if($taskCount -lt 1 -or $taskCount -gt 96 -or $taskOptional -lt 112 -or $taskPe+24+$taskOptional+40*$taskCount -gt 4096){throw 'Unsupported section directory'}
  $taskStream.Position=$taskPe+24+$taskOptional
  $taskSections=@()
  for($taskI=0;$taskI -lt $taskCount;$taskI++){
    $taskRow=$taskReader.ReadBytes(40)
    $taskSection=[ordered]@{
      name=[Text.Encoding]::ASCII.GetString($taskRow,0,8).Trim([char]0)
      rva=[long][BitConverter]::ToUInt32($taskRow,12)
      virtualBytes=[long][BitConverter]::ToUInt32($taskRow,8)
      rawOffset=[long][BitConverter]::ToUInt32($taskRow,20)
      rawBytes=[long][BitConverter]::ToUInt32($taskRow,16)
      flags=[BitConverter]::ToUInt32($taskRow,36)
    }
    if($taskSection.rawOffset -gt $taskStream.Length-$taskSection.rawBytes -or $taskSection.rva -gt [long]$taskSize-$taskSection.virtualBytes){throw 'Section escapes the image'}
    $taskSections+=,$taskSection
  }
  foreach($taskDefinition in $taskDefinitions){
    $taskContaining=@($taskSections|Where-Object {$taskDefinition.rva -ge $_.rva -and $taskDefinition.rva+$taskDefinition.bytes.Length -le $_.rva+$_.rawBytes})
    if($taskContaining.Count -ne 1){$taskDefinition.oldRvaMatches=$false;continue}
    $taskStream.Position=$taskContaining[0].rawOffset+$taskDefinition.rva-$taskContaining[0].rva
    $taskActual=$taskReader.ReadBytes($taskDefinition.bytes.Length)
    $taskDefinition.oldRvaMatches=([Convert]::ToBase64String($taskActual) -eq [Convert]::ToBase64String($taskDefinition.bytes))
  }
  foreach($taskSection in $taskSections){
    if($taskSection.rawBytes -eq 0 -or ($taskSection.flags -band 0x80000000L)){continue}
    $taskExecutable=($taskSection.flags -band 0x20000000) -ne 0
    $taskSelected=@($taskDefinitions|Where-Object {($_.scope -eq 'code') -eq $taskExecutable})
    if(-not $taskSelected.Count){continue}
    $taskPatterns=[byte[][]]@($taskSelected|ForEach-Object {,$_.bytes})
    $taskFound=[LauncherDiskPatternAudit]::Find($taskStream,$taskSection.rawOffset,$taskSection.rawBytes,$taskPatterns,1048576)
    for($taskP=0;$taskP -lt $taskSelected.Count;$taskP++){
      foreach($taskAt in $taskFound[$taskP]){
        $taskSelected[$taskP].hits.Add(@{section=$taskSection.name;rva=('{0:X8}' -f ($taskSection.rva+$taskAt-$taskSection.rawOffset));fileOffset=('{0:X8}' -f $taskAt)})
      }
    }
  }
  $taskStream.Position=0
  $taskHasher=[Security.Cryptography.SHA256]::Create()
  try {$taskHash=[BitConverter]::ToString($taskHasher.ComputeHash($taskStream)).Replace('-','')} finally {$taskHasher.Dispose()}
  $taskAfter=Get-Item -LiteralPath $taskGame
  if($taskBefore.Length -ne $taskStream.Length -or $taskBefore.LastWriteTimeUtc -ne $taskAfter.LastWriteTimeUtc){throw 'Executable changed during audit'}
  $taskReport=[ordered]@{
    schema='nba2k27-disk-anchor-audit-v1';utc=[DateTime]::UtcNow.ToString('o');path=$taskGame
    bytes=$taskStream.Length;sha256=$taskHash;timestamp=('{0:X8}' -f $taskStamp);sizeOfImage=('{0:X8}' -f $taskSize);preferredBase=('{0:X}' -f $taskBase)
    gameProcessAccess=$false;gameFilesChanged=$false;buildGuardsChanged=$false;candidateHitsAreNotVerifiedPointers=$true
    excludedWritableOrRuntimePackedSections=$true;sections=$taskSections
    anchors=@($taskDefinitions|ForEach-Object {@{name=$_.name;oldRva=('{0:X8}' -f $_.rva);oldRvaMatches=$_.oldRvaMatches;pattern=$_.hex;hits=@($_.hits.ToArray());status=$(if($_.hits.Count -eq 1){'unique-disk-candidate'}elseif($_.hits.Count -eq 0){'not-found-in-readonly-disk-sections'}else{'ambiguous-disk-candidates'})}})
  }
} finally {$taskStream.Dispose()}
if($OutputPath){
  $taskArtifacts=[IO.Path]::GetFullPath((Join-Path (Split-Path $PSScriptRoot -Parent) 'artifacts'))+[IO.Path]::DirectorySeparatorChar
  $taskOutput=[IO.Path]::GetFullPath($OutputPath)
  if(-not $taskOutput.StartsWith($taskArtifacts,[StringComparison]::OrdinalIgnoreCase)){throw 'Diagnostic output must stay in project artifacts'}
  $taskJson=$taskReport|ConvertTo-Json -Depth 12
  $taskOutputStream=[IO.File]::Open($taskOutput,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
  try {
    $taskBytes=[Text.UTF8Encoding]::new($false).GetBytes($taskJson)
    $taskOutputStream.Write($taskBytes,0,$taskBytes.Length)
  } finally {$taskOutputStream.Dispose()}
}
[pscustomobject]@{timestamp=$taskReport.timestamp;sizeOfImage=$taskReport.sizeOfImage;sha256=$taskReport.sha256;anchors=$taskReport.anchors;output=$OutputPath;gameProcessAccess=$false}|ConvertTo-Json -Depth 8
