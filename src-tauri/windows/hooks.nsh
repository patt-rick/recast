; Recast's uninstall cleanup (spec 6.12). Tauri's template already removes the Run value
; named after the product ("Recast"); this also removes the Explorer verb and Recast's data.
; Nothing here runs during an update (/UPDATE) that reinstalls over the old version.
; Tauri's reinstall page runs the old uninstaller WITHOUT /UPDATE on a manual upgrade, so
; engines and history (%LOCALAPPDATA%\Recast) and settings (%APPDATA%\Recast) are removed
; only when "Delete app data" is ticked. Logs and temp files are always removed.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    DeleteRegKey HKCU "Software\Classes\*\shell\Recast"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Recast"
    SetShellVarContext current
    RMDir /r "$LOCALAPPDATA\Recast\logs"
    RMDir /r "$LOCALAPPDATA\Recast\tmp"
    ${If} $DeleteAppDataCheckboxState = 1
      RMDir /r "$LOCALAPPDATA\Recast"
      RMDir /r "$APPDATA\Recast"
    ${EndIf}
  ${EndIf}
!macroend
