!macro NSIS_HOOK_PREINSTALL
  nsExec::ExecToLog '"$SYSDIR\sc.exe" stop HaCompanionSensors'
  Pop $0
  Sleep 1500
!macroend

!macro NSIS_HOOK_POSTINSTALL
  MessageBox MB_YESNO|MB_DEFBUTTON2|MB_ICONQUESTION "Install the optional CPU temperature provider? This adds the included Microsoft-signed PawnIO driver and the Home Assistant Companion sensor service. Then enable CPU temperature in the app settings. Windows administrator approval is required." /SD IDNO IDYES sensor_install_driver IDNO sensor_install_refresh

sensor_install_driver:
  nsExec::ExecToLog '"$INSTDIR\resources\sensor-service\ha-companion-sensor-service.exe" --install-with-driver'
  Pop $0
  StrCmp $0 "0" sensor_install_done
  MessageBox MB_OK|MB_ICONSTOP "Could not install the optional sensor driver or service (exit code $0). CPU temperature will remain unknown." /SD IDOK
  Goto sensor_install_done

sensor_install_refresh:
  nsExec::ExecToLog '"$INSTDIR\resources\sensor-service\ha-companion-sensor-service.exe" --refresh-existing'
  Pop $0
  StrCmp $0 "0" sensor_install_done
  MessageBox MB_OK|MB_ICONSTOP "Could not restart the existing sensor service (exit code $0). CPU temperature will remain unknown." /SD IDOK

sensor_install_done:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog '"$INSTDIR\resources\sensor-service\ha-companion-sensor-service.exe" --uninstall'
  Pop $0
  StrCmp $0 "0" sensor_uninstall_done
  Abort "Could not remove the sensor service before uninstalling."
sensor_uninstall_done:
!macroend
