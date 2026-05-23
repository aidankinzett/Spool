; Inno Setup Compiler script for Ludusavi Wrap
; To build: iscc installer.iss

#define AppExe "bin\Release\net9.0-windows\win-x64\publish\ludusavi-wrap.exe"
#define AppVersion GetFileVersion(AppExe)

[Setup]
AppName=Ludusavi Wrap
AppVersion={#AppVersion}
DefaultDirName={userpf}\Ludusavi Wrap
DefaultGroupName=Ludusavi Wrap
UninstallDisplayIcon={app}\ludusavi-wrap.exe
Compression=lzma2
SolidCompression=yes
OutputDir=dist
OutputBaseFilename=ludusavi-wrap-setup
PrivilegesRequired=lowest
DisableProgramGroupPage=yes
DisableDirPage=auto
DisableReadyPage=yes

[Files]
Source: "bin\Release\net9.0-windows\win-x64\publish\ludusavi-wrap.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Ludusavi Wrap"; Filename: "{app}\ludusavi-wrap.exe"
Name: "{userdesktop}\Ludusavi Wrap"; Filename: "{app}\ludusavi-wrap.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop icon"; GroupDescription: "Additional icons:"; Flags: unchecked

[Run]
Filename: "{app}\ludusavi-wrap.exe"; Description: "Launch Ludusavi Wrap"; Flags: postinstall nowait
