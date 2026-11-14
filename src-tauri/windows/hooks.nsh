; Installer hooks for the Windows (NSIS) bundle.
;
; The environment is chosen once, during installation, and is written to
; %ProgramData%\EcoAIOps\install-mode.json. The app treats that file as a lock:
; it overrides the profile mode and refuses runtime switching.
;
; Silent install: pass /MODE=local or /MODE=corporate (the ECO_INSTALL_MODE
; environment variable is also read as a fallback). Without either, a silent
; install defaults to local, matching the first-run dialog.

!macro NSIS_HOOK_PREINSTALL
  StrCpy $R8 "local"
  ${GetOptions} $CMDLINE "/MODE=" $R9
  ReadEnvStr $R7 "ECO_INSTALL_MODE"
  StrCmp $R9 "" 0 +2
  StrCpy $R9 $R7
  StrCmp $R9 "corporate" 0 +2
  StrCpy $R8 "corporate"
  IfSilent EcoPlatformModeReady
  MessageBox MB_YESNO|MB_ICONQUESTION "Установить ECO AIOps в локальном режиме?$\r$\n$\r$\nДа — локальный: любые пакеты, без trust anchor и allowlist.$\r$\nНет — корпоративный: только пакеты, подписанные доверенным ключом и разрешённые allowlist." IDYES EcoPlatformModeReady
  StrCpy $R8 "corporate"
EcoPlatformModeReady:
  ReadEnvStr $R7 "ProgramData"
  StrCmp $R7 "" 0 +2
  StrCpy $R7 "$WINDIR\..\ProgramData"
  CreateDirectory "$R7\EcoAIOps"
  FileOpen $R9 "$R7\EcoAIOps\install-mode.json" w
  FileWrite $R9 '{"environment":"'
  FileWrite $R9 $R8
  FileWrite $R9 '"}'
  FileClose $R9
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ReadEnvStr $R7 "ProgramData"
  StrCmp $R7 "" 0 +2
  StrCpy $R7 "$WINDIR\..\ProgramData"
  Delete "$R7\EcoAIOps\install-mode.json"
  RMDir "$R7\EcoAIOps"
!macroend
