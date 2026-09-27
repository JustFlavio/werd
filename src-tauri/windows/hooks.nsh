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
  ; The default per-user folder, $LOCALAPPDATA\Werd, is also Werd's data folder.
  ; Keep programs apart, like other per-user apps, and remove the programs an
  ; older installer put among the data. A folder the user picked is kept.
  ${If} $INSTDIR == "$LOCALAPPDATA\Werd"
    StrCpy $INSTDIR "$LOCALAPPDATA\Programs\Werd"
    SetOutPath $INSTDIR
    Delete "$LOCALAPPDATA\Werd\werd-desktop.exe"
    Delete "$LOCALAPPDATA\Werd\werd.exe"
    Delete "$LOCALAPPDATA\Werd\werd-daemon.exe"
    Delete "$LOCALAPPDATA\Werd\werd-shim.exe"
    Delete "$LOCALAPPDATA\Werd\werd-helper.exe"
    Delete "$LOCALAPPDATA\Werd\uninstall.exe"
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; Keep the user's setup (PATH, .test domains, launch at login) unless this is
  ; a real uninstall. Installing a new version runs the old uninstaller straight
  ; from the program folder (with _?=), and the updater also adds /UPDATE. An
  ; uninstall started from Windows Settings first copies itself to a temporary
  ; folder, so only then does $EXEDIR differ from $INSTDIR.
  StrCpy $R9 0
  ${If} $UpdateMode <> 1
  ${AndIf} $EXEDIR != $INSTDIR
    StrCpy $R9 1
  ${EndIf}
  ${If} $R9 = 1
    ; Remove Werd's bin folder from the user PATH.
    nsExec::Exec '"$INSTDIR\werd.exe" path disable'
    Pop $0
  ${EndIf}
  !insertmacro WERD_STOP_DAEMON
  ${If} $R9 = 1
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
