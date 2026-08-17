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
; - ForceQuitRunningApp retries kill+FindProcess so a dying tray process is not
;   mistaken for "still running" (Tauri #12309). In-app updates must never show
;   "Please close it first then try again."

!macro ForceQuitRunningApp
  !define ForceQuitID ${__LINE__}
  StrCpy $R8 0
  force_quit_loop_${ForceQuitID}:
    nsis_tauri_utils::KillProcessCurrentUser "${MAINBINARYNAME}.exe"
    Pop $R9
    ; Ignore kill errors: the process may already have exited (TOCTOU).
    Sleep 450
    nsis_tauri_utils::FindProcessCurrentUser "${MAINBINARYNAME}.exe"
    Pop $R7
    ; FindProcess returns 0 when a matching process is still running.
    ${If} $R7 <> 0
      Goto force_quit_done_${ForceQuitID}
    ${EndIf}
    IntOp $R8 $R8 + 1
    ${If} $R8 < 16
      Goto force_quit_loop_${ForceQuitID}
    ${EndIf}
  force_quit_done_${ForceQuitID}:
  !undef ForceQuitID
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; Unlock the binary before file copy. In-app update quits then races NSIS;
  ; retry until maxspeech.exe is gone so CheckIfAppIsRunning never prompts.
  !insertmacro ForceQuitRunningApp
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Async launch — do not use RunAsUser here (blocks until Quit).
  ; Silent/passive/update: always start so in-app updater and /S feel finished.
  ; Interactive wizard: also start once so a fresh PC sees MaxSpeech after Next.
  System::Call 'shell32::ShellExecuteW(i 0, w "open", w "$INSTDIR\${MAINBINARYNAME}.exe", i 0, w "$INSTDIR", i 1)'
!macroend
