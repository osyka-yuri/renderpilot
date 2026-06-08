; RenderPilot NSIS installer hooks
;
; Tauri's generated uninstall section deletes $LOCALAPPDATA\${BUNDLEID} and
; $APPDATA\${BUNDLEID}, where BUNDLEID = "com.renderpilot.desktop". But Tauri
; actually stores app data under the product name ("RenderPilot"), so those
; paths never match. This hook runs after the built-in cleanup block and removes
; the real directories when the user has checked "Delete app data".

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    RmDir /r "$LOCALAPPDATA\RenderPilot"
    RmDir /r "$APPDATA\RenderPilot"
  ${EndIf}
!macroend
