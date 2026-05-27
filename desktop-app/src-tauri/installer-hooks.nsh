; Tauri NSIS hook — register the WinRing0 kernel driver as a Windows
; service during install (the NSIS installer is already elevated, so this
; is the right moment). After install, the runtime ha-companion.exe just
; opens \\.\WinRing0_1_2_0 — no admin needed, no UAC prompt each launch.
;
; ${PROJECTDIR} is provided by Tauri's NSIS template and points at
; desktop-app/src-tauri/, so the driver path is fully qualified at
; NSIS-compile time.

!macro NSIS_HOOK_POSTINSTALL
    ; Drop the bundled driver into System32\drivers. Overwrite is safe —
    ; if another tool installed an identical-or-newer copy it'll still
    ; work; if the file is locked by an active driver, NSIS retries.
    SetOutPath "$SYSDIR\drivers"
    File "${PROJECTDIR}\drivers\WinRing0x64.sys"

    ; Only create the service if it doesn't exist (some users already have
    ; it from LHM/HWiNFO/CoreTemp/etc — leave those alone, just use them).
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
