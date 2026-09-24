!macro NSIS_HOOK_PREINSTALL
  nsExec::ExecToLog '"$SYSDIR\sc.exe" stop HaCompanionSensors'
  Pop $0
  Sleep 1500
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; The service runs as LocalSystem. Tauri's default per-machine folder can
  ; inherit a full-control ACE for the installing user, so lock the entire
  ; installation tree before registering or restarting the service.
  nsExec::ExecToLog '"$SYSDIR\icacls.exe" "$INSTDIR" /reset /T'
  Pop $0
  StrCmp $0 "0" sensor_acl_reset_ok
  Abort "Could not reset application permissions (exit code $0)."
sensor_acl_reset_ok:
  nsExec::ExecToLog '"$SYSDIR\icacls.exe" "$INSTDIR" /inheritance:r'
  Pop $0
  StrCmp $0 "0" sensor_acl_inheritance_ok
  Abort "Could not remove inherited application permissions (exit code $0)."
sensor_acl_inheritance_ok:
  nsExec::ExecToLog '"$SYSDIR\icacls.exe" "$INSTDIR" /grant "*S-1-5-18:(OI)(CI)F" "*S-1-5-32-544:(OI)(CI)F" "*S-1-5-32-545:(OI)(CI)RX"'
  Pop $0
  StrCmp $0 "0" sensor_acl_grant_ok
  Abort "Could not secure application permissions (exit code $0)."
sensor_acl_grant_ok:
  nsExec::ExecToLog '"$SYSDIR\icacls.exe" "$INSTDIR" /setowner "*S-1-5-32-544" /T'
  Pop $0
  StrCmp $0 "0" sensor_acl_owner_ok
  Abort "Could not secure application ownership (exit code $0)."
sensor_acl_owner_ok:
  MessageBox MB_YESNO|MB_DEFBUTTON2|MB_ICONQUESTION "Install the optional CPU temperature provider? This adds the included Microsoft-signed PawnIO driver and the Home Assistant Companion sensor service. Then enable CPU temperature in the app settings. Windows administrator approval is required." /SD IDNO IDYES sensor_install_driver IDNO sensor_install_refresh

sensor_install_driver:
  nsExec::ExecToLog '"$INSTDIR\sensor-service\ha-companion-sensor-service.exe" --install-with-driver'
  Pop $0
  StrCmp $0 "0" sensor_install_done
  MessageBox MB_OK|MB_ICONSTOP "Could not install the optional sensor driver or service (exit code $0). CPU temperature will remain unknown." /SD IDOK
  Goto sensor_install_done

sensor_install_refresh:
  nsExec::ExecToLog '"$INSTDIR\sensor-service\ha-companion-sensor-service.exe" --refresh-existing'
  Pop $0
  StrCmp $0 "0" sensor_install_done
  MessageBox MB_OK|MB_ICONSTOP "Could not restart the existing sensor service (exit code $0). CPU temperature will remain unknown." /SD IDOK

sensor_install_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog '"$INSTDIR\sensor-service\ha-companion-sensor-service.exe" --uninstall'
  Pop $0
  StrCmp $0 "0" sensor_uninstall_done
  Abort "Could not remove the sensor service before uninstalling."
sensor_uninstall_done:
!macroend
