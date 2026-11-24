; Hooks del instalador NSIS (Tauri: bundle.windows.nsis.installerHooks).
;
; cs-bridge.exe lo lanza Zen (Native Messaging) y queda en uso mientras Zen está abierto:
; sin esto, una actualización NO podía reemplazarlo y quedaba el puente viejo sin avisar.
; Se cierra justo antes de copiar; la extensión lo vuelve a lanzar sola (reintenta en ~1 s).

!macro NSIS_HOOK_PREINSTALL
  nsExec::Exec 'taskkill /F /IM cs-bridge.exe'
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /F /IM cs-bridge.exe'
  Pop $0
!macroend
