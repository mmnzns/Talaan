; Talaan's NSIS hooks (bundle.windows.nsis.installerHooks in tauri.windows.conf.json).
; Tauri's uninstaller, when "Delete the application data" is ticked, removes only %APPDATA% and %LOCALAPPDATA%
; folders named after the bundle id. Talaan keeps meetings and settings in %APPDATA%\Talaan and API keys in
; Credential Manager, so those go here too. Models are in %LOCALAPPDATA%\<bundle id>\models since v1.1 (Tauri's
; own RmDir covers it; removed here as well so it doesn't depend on the template), or still in %APPDATA%\Talaan\models
; when the move hadn't finished. Never inside $INSTDIR, which Tauri empties file by file. Updates (/UPDATE) never
; delete anything.

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

; talaan:// links (src-tauri/src/deeplink.rs). A "Summary ready" toast opens talaan://meeting/<id>, so a click
; reaches Talaan from Action Center too (no COM activator needed). Per user, like the install. Rewritten by every
; install and update, so it always points at this install's exe.
!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Classes\talaan" "" "URL:Talaan"
  WriteRegStr HKCU "Software\Classes\talaan" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\talaan\DefaultIcon" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\",0"
  WriteRegStr HKCU "Software\Classes\talaan\shell\open\command" "" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\" $\"%1$\""
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro TalaanCloseHelpers
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ; Tauri removes the Run value; auto-launch also wrote this one under the same name.
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "Talaan"
    ; The talaan:// links would point at a removed exe (an update keeps them; its install rewrites them).
    DeleteRegKey HKCU "Software\Classes\talaan"
    ${If} $DeleteAppDataCheckboxState = 1
      SetShellVarContext current
      RmDir /r "$APPDATA\Talaan"
      RmDir /r "$LOCALAPPDATA\${BUNDLEID}\models"
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
