; Tauri NSIS hook — register the WinRing0 kernel driver as a Windows
; service during install (the NSIS installer is already elevated). After
; install, the runtime ha-companion.exe just opens \\.\WinRing0_1_2_0 —
; no admin needed, no UAC prompt each launch.
;
; The driver file is bundled via `bundle.resources` in tauri.conf.json,
; so by the time POSTINSTALL fires Tauri has already extracted it to
; $INSTDIR\resources\drivers\WinRing0x64.sys.

!macro NSIS_HOOK_POSTINSTALL
    ; Make sure target dir exists, then copy the bundled driver there.
    CreateDirectory "$SYSDIR\drivers"
    CopyFiles /SILENT "$INSTDIR\resources\drivers\WinRing0x64.sys" "$SYSDIR\drivers"

    ; Only create the service if it doesn't exist (some users already
    ; have it from LHM/HWiNFO/CoreTemp/etc — leave those alone).
    nsExec::Exec 'sc.exe query WinRing0_1_2_0'
    Pop $0
    StrCmp $0 "0" service_exists service_create

    service_create:
        nsExec::Exec 'sc.exe create WinRing0_1_2_0 binPath= "$SYSDIR\drivers\WinRing0x64.sys" type= kernel start= auto error= normal DisplayName= "WinRing0_1_2_0"'

    service_exists:
        ; Start it so the device is ready before the app first runs.
        ; No-op if already running.
        nsExec::Exec 'sc.exe start WinRing0_1_2_0'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
    nsExec::Exec 'sc.exe stop WinRing0_1_2_0'
    nsExec::Exec 'sc.exe delete WinRing0_1_2_0'
    Delete "$SYSDIR\drivers\WinRing0x64.sys"
!macroend
