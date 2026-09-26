; Werd installer hooks, included by the Tauri NSIS template.
;
; The daemon runs outside the app window, so it must be stopped before its files
; are replaced or removed. Stopping it also stops PHP, Caddy and every service
; (they share its job object). User data in %LOCALAPPDATA%\Werd is kept.

!macro WERD_STOP_DAEMON
  nsExec::Exec '"$SYSDIR\taskkill.exe" /F /IM werd-daemon.exe'
  Pop $0
  Sleep 500
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro WERD_STOP_DAEMON
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; An update runs the old uninstaller with /UPDATE: keep the user's setup then.
  ${If} $UpdateMode <> 1
    ; Remove Werd's bin folder from the user PATH.
    nsExec::Exec '"$INSTDIR\werd.exe" path disable'
    Pop $0
  ${EndIf}
  !insertmacro WERD_STOP_DAEMON
  ${If} $UpdateMode <> 1
    ; Remove the .test domains from the hosts file. This needs administrator
    ; approval, so ask only when the file has a Werd block.
    nsExec::Exec '"$INSTDIR\werd-helper.exe" check'
    Pop $0
    ${If} $0 == 1
      ExecShellWait "runas" "$INSTDIR\werd-helper.exe" "hosts" SW_HIDE
    ${EndIf}
    ; Launch at login (tauri-plugin-autostart, named "Werd").
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Werd"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Werd"
  ${EndIf}
!macroend
