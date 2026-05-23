; Inno Setup Compiler script for Ludusavi Wrap
; To build: iscc installer.iss

[Setup]
AppName=Ludusavi Wrap
AppVersion=1.0.3
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
Source: "dist\ludusavi-wrap.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Ludusavi Wrap"; Filename: "{app}\ludusavi-wrap.exe"
Name: "{userdesktop}\Ludusavi Wrap"; Filename: "{app}\ludusavi-wrap.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop icon"; GroupDescription: "Additional icons:"; Flags: unchecked

[Run]
Filename: "{app}\ludusavi-wrap.exe"; Description: "Launch Ludusavi Wrap"; Flags: postinstall nowait
