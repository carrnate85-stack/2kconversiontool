#ifndef SourceDir
  #error SourceDir must be supplied by build_release.ps1
#endif
#ifndef AppVersion
  #define AppVersion "1.0.123-beta"
#endif
#ifndef OutputDir
  #define OutputDir "."
#endif

[Setup]
AppId={{A521C27D-1109-45D1-9C02-F610AC56A64C}
AppName=Character Mod Tool
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\Character Mod Tool
DefaultGroupName=Character Mod Tool
OutputDir={#OutputDir}
OutputBaseFilename=CharacterModTool-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Character Mod Tool"; Filename: "{app}\CharacterModTool.exe"
Name: "{autodesktop}\Character Mod Tool"; Filename: "{app}\CharacterModTool.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"

[Run]
Filename: "{app}\CharacterModTool.exe"; Description: "Launch Character Mod Tool"; Flags: nowait postinstall skipifsilent
