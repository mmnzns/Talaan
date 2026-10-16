; Talaan's NSIS hooks (bundle.windows.nsis.installerHooks in tauri.windows.conf.json).
; Tauri's uninstaller, when "Delete the application data" is ticked, removes only %APPDATA% and %LOCALAPPDATA%
; folders named after the bundle id. Talaan keeps meetings, models and settings in %APPDATA%\Talaan and API keys
; in Credential Manager, so those go here too. Updates (/UPDATE) never delete anything.

; One Credential Manager entry: keyring writes generic credentials (type 1) named "<account>.Talaan".
; The list below must match keys::ACCOUNTS (src-tauri/src/keys.rs); a test there checks it.
!macro TalaanDeleteKey ACCOUNT
  System::Call 'advapi32::CredDeleteW(w "${ACCOUNT}.Talaan", i 1, i 0) i .r0'
!macroend

; The helpers can outlive Talaan for a few seconds; close them so their exes aren't locked. The GPU helpers
; (talaan-llm-vulkan.exe, talaan-stt-vulkan.exe) are only in installers built with the Vulkan SDK; checking for
; one that isn't running is a no-op.
!macro TalaanCloseHelpers
  !insertmacro CheckIfAppIsRunning "$INSTDIR\talaan-llm.exe" "${PRODUCTNAME}"
  !insertmacro CheckIfAppIsRunning "$INSTDIR\talaan-llm-vulkan.exe" "${PRODUCTNAME}"
  !insertmacro CheckIfAppIsRunning "$INSTDIR\talaan-stt-vulkan.exe" "${PRODUCTNAME}"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro TalaanCloseHelpers
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro TalaanCloseHelpers
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ; Tauri removes the Run value; auto-launch also wrote this one under the same name.
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Talaan"
    ${If} $DeleteAppDataCheckboxState = 1
      SetShellVarContext current
      RmDir /r "$APPDATA\Talaan"
      !insertmacro TalaanDeleteKey "claude"
      !insertmacro TalaanDeleteKey "openai"
      !insertmacro TalaanDeleteKey "gemini"
      !insertmacro TalaanDeleteKey "groq"
      !insertmacro TalaanDeleteKey "cerebras"
      !insertmacro TalaanDeleteKey "mistral"
      !insertmacro TalaanDeleteKey "openrouter"
      !insertmacro TalaanDeleteKey "compatible"
      !insertmacro TalaanDeleteKey "notion"
      !insertmacro TalaanDeleteKey "calendar-ics"
    ${EndIf}
  ${EndIf}
!macroend
