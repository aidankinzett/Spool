; Windows Defender Firewall rule for LAN sharing.
;
; LAN discovery listens on UDP 47631 and the game server on a TCP port that
; can be changed in Settings, so the rule is for the program rather than for
; fixed ports. Private and Domain profiles only: a network Windows has
; classed as Public (cafe or hotel Wi-Fi) stays closed.
;
; The installer runs per-user, so adding or removing a rule needs a UAC
; prompt (`runas`). The rule is only added when none points at this
; install's spool.exe, so the prompt appears on the first install, after
; moving the install folder, or on the first update to a version with this
; hook, not on every update. A rule left for an old install folder is not
; deleted, since it can't be told apart from one the user made. Declining the
; prompt leaves Spool working, with LAN sharing blocked until Windows' own
; firewall prompt is accepted.

!macro NSIS_HOOK_POSTINSTALL
  ; findstr exits 0 only if a rule named Spool lists this exe's path.
  nsExec::ExecToStack '"$SYSDIR\cmd.exe" /c netsh advfirewall firewall show rule name=Spool verbose | findstr /i /c:"$INSTDIR\spool.exe"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    ExecShellWait "runas" "$SYSDIR\cmd.exe" '/c netsh advfirewall firewall add rule name="Spool" dir=in action=allow program="$INSTDIR\spool.exe" enable=yes profile=private,domain' SW_HIDE
  ${EndIf}
!macroend

; The uninstaller also runs when an update replaces the old version (with
; /UPDATE, so $UpdateMode is 1). Leave the rule alone then, or every update
; would prompt for elevation to delete it and again to add it back. Only the
; rule for this install's exe is removed, so a rule another install or the
; user named "Spool" survives.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::ExecToStack '"$SYSDIR\cmd.exe" /c netsh advfirewall firewall show rule name=Spool verbose | findstr /i /c:"$INSTDIR\spool.exe"'
    Pop $0
    Pop $1
    ${If} $0 == 0
      ExecShellWait "runas" "$SYSDIR\cmd.exe" '/c netsh advfirewall firewall delete rule name="Spool" program="$INSTDIR\spool.exe"' SW_HIDE
    ${EndIf}
  ${EndIf}
!macroend
