; MaxSpeech NSIS hooks
;
; Critical: nsis_tauri_utils::RunAsUser waits for the process to exit. MaxSpeech
; is a tray app, so RunAsUser hangs the installer forever (silent /R and the
; finish-page "Run" checkbox). Users then kill the installer mid-flow after an
; upgrade's uninstall-first step — leaving a broken / missing install.
;
; Fix:
; - Custom windows/installer.nsi removes finish-page Run entirely and makes /R
;   use async ShellExecute.
; - This POSTINSTALL hook also launches asynchronously so /S /UPDATE (in-app
;   updater; no /R) still starts the app after install.

!macro NSIS_HOOK_PREINSTALL
  ; Unlock the binary before file copy (in-app update exits then races NSIS).
  ; CheckIfAppIsRunning also kills, but silent updates need an early unlock.
  nsis_tauri_utils::KillProcessCurrentUser "${MAINBINARYNAME}.exe"
  Pop $R9
  Sleep 700
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Async launch — do not use RunAsUser here (blocks until Quit).
  ; Silent/passive/update: always start so in-app updater and /S feel finished.
  ; Interactive wizard: also start once so a fresh PC sees MaxSpeech after Next.
  System::Call 'shell32::ShellExecuteW(i 0, w "open", w "$INSTDIR\${MAINBINARYNAME}.exe", i 0, w "$INSTDIR", i 1)'
!macroend
