; Moving over from the older Hearth (built with Electron): install into its folder, so its shortcuts and its
; "update and restart" (which starts hearth.exe in that folder) open this version. Its own files are removed first.
!macro NSIS_HOOK_PREINSTALL
  StrCpy $0 ""
  IfFileExists "$LOCALAPPDATA\Programs\hearth-desktop\resources\app.asar" 0 +2
    StrCpy $0 "$LOCALAPPDATA\Programs\hearth-desktop"
  IfFileExists "$LOCALAPPDATA\Programs\Hearth\resources\app.asar" 0 +2
    StrCpy $0 "$LOCALAPPDATA\Programs\Hearth"
  StrCmp $0 "" hearth_no_old
    StrCpy $INSTDIR $0
    RMDir /r "$INSTDIR\resources"
    RMDir /r "$INSTDIR\locales"
    Delete "$INSTDIR\*.pak"
    Delete "$INSTDIR\*.dll"
    Delete "$INSTDIR\*.bin"
    Delete "$INSTDIR\*.dat"
    Delete "$INSTDIR\*.json"
    Delete "$INSTDIR\LICENSE*"
    Delete "$INSTDIR\Uninstall Hearth.exe"
  hearth_no_old:
!macroend
