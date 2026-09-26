; Runs as admin (perMachine install), so the driver and firewall steps need no extra prompts.
!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Liberando o TabDisplay no firewall..."
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="TabDisplay"'
  Pop $0
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="TabDisplay" dir=in action=allow program="$INSTDIR\tabdisplay.exe" enable=yes profile=any'
  Pop $0
  DetailPrint "Instalando o monitor virtual (Virtual Display Driver)..."
  nsExec::ExecToLog '"$INSTDIR\tabdisplay.exe" --install-driver'
  Pop $0
  DetailPrint "Monitor virtual: código $0 (0 = ok)"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog '"$INSTDIR\tabdisplay.exe" --uninstall-driver'
  Pop $0
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="TabDisplay"'
  Pop $0
!macroend
