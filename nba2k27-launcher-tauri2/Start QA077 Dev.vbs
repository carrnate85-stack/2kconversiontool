Option Explicit
Dim shell, files, folder, powershell, command
Set shell = CreateObject("WScript.Shell")
Set files = CreateObject("Scripting.FileSystemObject")
folder = files.GetParentFolderName(WScript.ScriptFullName)
powershell = shell.ExpandEnvironmentStrings("%SystemRoot%") & "\System32\WindowsPowerShell\v1.0\powershell.exe"
command = """" & powershell & """ -NoLogo -NoProfile -ExecutionPolicy Bypass -File """ & folder & "\Start QA077 Dev.ps1"""
If WScript.Arguments.Named.Exists("check") Then
  WScript.Echo command
Else
  shell.Run command, 0, False
End If
