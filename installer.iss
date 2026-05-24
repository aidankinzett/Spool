; Inno Setup Compiler script for Ludusavi Wrap
; To build: iscc installer.iss

#define AppExe "bin\Release\net9.0-windows10.0.17763.0\win-x64\publish\ludusavi-wrap.exe"
#define AppVersion GetVersionNumbersString(AppExe)

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
Source: "bin\Release\net9.0-windows10.0.17763.0\win-x64\publish\ludusavi-wrap.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Ludusavi Wrap"; Filename: "{app}\ludusavi-wrap.exe"
Name: "{userdesktop}\Ludusavi Wrap"; Filename: "{app}\ludusavi-wrap.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop icon"; GroupDescription: "Additional icons:"; Flags: unchecked

[Run]
; Shown as checkbox on finish page for manual installs; skipped during silent auto-update
Filename: "{app}\ludusavi-wrap.exe"; Description: "Launch Ludusavi Wrap"; Flags: postinstall nowait skipifsilent
; Relaunch automatically after a silent auto-update
Filename: "{app}\ludusavi-wrap.exe"; Flags: nowait; Check: WizardSilent
