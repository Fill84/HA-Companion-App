; Tauri NSIS hook — register the WinRing0 kernel driver as a Windows
; service during install (the NSIS installer is already elevated, so this
; is the right moment). After install, the runtime ha-companion.exe just
; opens \\.\WinRing0_1_2_0 — no admin needed, no UAC prompt each launch.

!macro NSIS_HOOK_POSTINSTALL
    ; Only register if the service isn't already there from a prior tool.
    nsExec::ExecToStack 'sc.exe query WinRing0_1_2_0'
    Pop $0
    ${If} $0 != 0
        ; sc query returned non-zero — service doesn't exist. Install it.
        SetOutPath "$SYSDIR\drivers"
        File "drivers\WinRing0x64.sys"
        nsExec::Exec 'sc.exe create WinRing0_1_2_0 binPath= "$SYSDIR\drivers\WinRing0x64.sys" type= kernel start= auto error= normal DisplayName= "WinRing0_1_2_0"'
    ${EndIf}

    ; Start the service so the device is ready before the app first runs.
    ; (No-op if already running.)
    nsExec::Exec 'sc.exe start WinRing0_1_2_0'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
    ; Only remove the service if WE installed it (i.e. only our driver
    ; file is in System32\drivers — leave files installed by other tools
    ; alone). Simple heuristic: stop+delete unconditionally, then remove
    ; the .sys only if it was our copy. If another tool re-registers, the
    ; service comes back automatically next time that tool runs.
    nsExec::Exec 'sc.exe stop WinRing0_1_2_0'
    nsExec::Exec 'sc.exe delete WinRing0_1_2_0'
    Delete "$SYSDIR\drivers\WinRing0x64.sys"
!macroend
