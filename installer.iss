; Inno Setup Compiler script for Spool
; To build: iscc installer.iss

#define BuildDir "bin\Release\net10.0-windows10.0.17763.0\win-x64\publish"
#define AppExe BuildDir + "\spool.exe"
#define AppVersion GetVersionNumbersString(AppExe)

[Setup]
AppName=Spool
AppVersion={#AppVersion}
DefaultDirName={userpf}\Spool
DefaultGroupName=Spool
UninstallDisplayIcon={app}\spool.exe
Compression=lzma2
SolidCompression=yes
OutputDir=dist
OutputBaseFilename=spool-setup
PrivilegesRequired=lowest
DisableProgramGroupPage=yes
DisableDirPage=auto
DisableReadyPage=yes

[Files]
Source: "{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Spool"; Filename: "{app}\spool.exe"
Name: "{userdesktop}\Spool"; Filename: "{app}\spool.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop icon"; GroupDescription: "Additional icons:"; Flags: unchecked

[Run]
; Shown as checkbox on finish page for manual installs; skipped during silent auto-update
Filename: "{app}\spool.exe"; Description: "Launch Spool"; Flags: postinstall nowait skipifsilent
; Relaunch automatically after a silent auto-update
Filename: "{app}\spool.exe"; Flags: nowait; Check: WizardSilent
