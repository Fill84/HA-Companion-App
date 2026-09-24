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
  nsExec::ExecToLog '"$INSTDIR\sensor-service\ha-companion-sensor-service.exe" --install-with-driver'
  Pop $0
  StrCmp $0 "0" sensor_install_done
  Abort "Could not install the bundled sensor driver or service (exit code $0)."

sensor_install_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog '"$INSTDIR\sensor-service\ha-companion-sensor-service.exe" --uninstall'
  Pop $0
  StrCmp $0 "0" sensor_uninstall_done
  Abort "Could not remove the sensor service before uninstalling."
sensor_uninstall_done:
!macroend
