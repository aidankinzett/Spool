; Windows Defender Firewall rule for LAN sharing.
;
; LAN discovery listens on UDP 47631 and the game server on a TCP port that
; can be changed in Settings, so the rule is for the program rather than for
; fixed ports. Private and Domain profiles only: a network Windows has
; classed as Public (cafe or hotel Wi-Fi) stays closed.
;
; The installer runs per-user, so adding or removing a rule needs a UAC
; prompt (`runas`). The rule is only (re)written when none points at this
; install's spool.exe, so the prompt appears on the first install, after
; moving the install folder, or on the first update to a version with this
; hook, not on every update. Declining it leaves Spool working, with
; LAN sharing blocked until Windows' own firewall prompt is accepted.

!macro NSIS_HOOK_POSTINSTALL
  ; findstr exits 0 only if a rule named Spool lists this exe's path.
  nsExec::ExecToStack '"$SYSDIR\cmd.exe" /c netsh advfirewall firewall show rule name=Spool verbose | findstr /i /c:"$INSTDIR\spool.exe"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    ExecShellWait "runas" "$SYSDIR\cmd.exe" '/c netsh advfirewall firewall delete rule name="Spool" & netsh advfirewall firewall add rule name="Spool" dir=in action=allow program="$INSTDIR\spool.exe" enable=yes profile=private,domain' SW_HIDE
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToStack 'netsh advfirewall firewall show rule name="Spool"'
  Pop $0
  Pop $1
  ${If} $0 == 0
    ExecShellWait "runas" "netsh" 'advfirewall firewall delete rule name="Spool"' SW_HIDE
  ${EndIf}
!macroend
