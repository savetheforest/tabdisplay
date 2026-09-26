; Runs as admin (perMachine install), so the driver, service and firewall steps need no extra prompts.

!macro NSIS_HOOK_PREINSTALL
  ; Upgrading: the service holds tabdisplay.exe open.
  nsExec::ExecToLog 'sc stop TabDisplay'
  Pop $0
  Sleep 1500
!macroend

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

  ; The service keeps the virtual monitor unplugged until a tablet connects (no UAC for the app).
  DetailPrint "Registrando o serviço do TabDisplay..."
  nsExec::ExecToLog 'sc create TabDisplay binPath= "\"$INSTDIR\tabdisplay.exe\" --service" start= auto DisplayName= "TabDisplay"'
  Pop $0
  nsExec::ExecToLog 'sc config TabDisplay binPath= "\"$INSTDIR\tabdisplay.exe\" --service" start= auto'
  Pop $0
  nsExec::ExecToLog 'sc description TabDisplay "Liga o monitor virtual do TabDisplay só enquanto um tablet está conectado."'
  Pop $0
  nsExec::ExecToLog 'sc failure TabDisplay reset= 86400 actions= restart/5000/restart/5000/restart/5000'
  Pop $0
  nsExec::ExecToLog 'sc start TabDisplay'
  Pop $0
  DetailPrint "Serviço: código $0 (0 = ok)"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::ExecToLog 'sc stop TabDisplay'
  Pop $0
  Sleep 1500
  nsExec::ExecToLog 'sc delete TabDisplay'
  Pop $0
  nsExec::ExecToLog '"$INSTDIR\tabdisplay.exe" --uninstall-driver'
  Pop $0
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="TabDisplay"'
  Pop $0
!macroend
