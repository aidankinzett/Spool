; Windows Defender Firewall rule for LAN sharing.
;
; LAN discovery listens on UDP 47631 and the game server on a TCP port that
; can be changed in Settings, so the rule is for the program rather than for
; fixed ports. Private and Domain profiles only: a network Windows has
; classed as Public (cafe or hotel Wi-Fi) stays closed.
;
; The installer runs per-user, so adding or removing a rule needs a UAC
; prompt (`runas`). The rule is only added when it doesn't exist, so the
; prompt appears on the first install (or the first update to a version with
; this hook), not on every update. Declining it leaves Spool working, with
; LAN sharing blocked until Windows' own firewall prompt is accepted.

!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToStack 'netsh advfirewall firewall show rule name="Spool"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    ExecShellWait "runas" "netsh" 'advfirewall firewall add rule name="Spool" dir=in action=allow program="$INSTDIR\spool.exe" enable=yes profile=private,domain' SW_HIDE
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
