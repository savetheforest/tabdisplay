; Runs as admin (perMachine install), so the driver, service and firewall steps need no extra prompts.
!include LogicLib.nsh

!macro TD_CHECK command what
  nsExec::ExecToLog '${command}'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "${what} falhou (código $0). A instalação foi interrompida."
    Abort
  ${EndIf}
!macroend

!macro TD_CHECK_OPTIONAL_SERVICE command what
  nsExec::ExecToLog '${command}'
  Pop $0
  ; 1060 = service not installed, which is valid on a clean install/uninstall.
  ${If} $0 != 0
  ${AndIf} $0 != 1060
    MessageBox MB_ICONSTOP "${what} falhou (código $0). A instalação foi interrompida."
    Abort
  ${EndIf}
!macroend

!macro TD_WAIT_STOPPED
  nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -Command "$deadline=(Get-Date).AddSeconds(20); do { $s=Get-Service -Name TabDisplay -ErrorAction SilentlyContinue; if ($null -eq $s -or $s.Status -eq [System.ServiceProcess.ServiceControllerStatus]::Stopped) { exit 0 }; Start-Sleep -Milliseconds 250 } while ((Get-Date) -lt $deadline); exit 1"'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "O serviço TabDisplay não parou no prazo. A instalação foi interrompida."
    Abort
  ${EndIf}
!macroend

!macro TD_WAIT_RUNNING
  nsExec::ExecToLog 'powershell.exe -NoProfile -NonInteractive -Command "$deadline=(Get-Date).AddSeconds(20); do { $s=Get-Service -Name TabDisplay -ErrorAction SilentlyContinue; if ($null -ne $s -and $s.Status -eq [System.ServiceProcess.ServiceControllerStatus]::Running) { exit 0 }; Start-Sleep -Milliseconds 250 } while ((Get-Date) -lt $deadline); exit 1"'
  Pop $0
  ${If} $0 != 0
    MessageBox MB_ICONSTOP "O serviço TabDisplay não iniciou no prazo. A instalação foi interrompida."
    Abort
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREINSTALL
  ; Upgrading: the service holds tabdisplay.exe open.
  !insertmacro TD_CHECK_OPTIONAL_SERVICE 'sc stop TabDisplay' 'Parar o serviço TabDisplay'
  !insertmacro TD_WAIT_STOPPED
!macroend

!macro NSIS_HOOK_POSTINSTALL
  DetailPrint "Liberando o TabDisplay no firewall..."
  !insertmacro TD_CHECK 'netsh advfirewall firewall delete rule name="TabDisplay"' 'Remover regra antiga do firewall'
  !insertmacro TD_CHECK 'netsh advfirewall firewall add rule name="TabDisplay" dir=in action=allow program="$INSTDIR\tabdisplay.exe" enable=yes profile=private,domain' 'Criar regra do firewall'

  DetailPrint "Instalando o monitor virtual (Virtual Display Driver)..."
  !insertmacro TD_CHECK '"$INSTDIR\tabdisplay.exe" --install-driver' 'Instalar o monitor virtual'
  DetailPrint "Monitor virtual: código $0 (0 = ok)"

  ; The service keeps the virtual monitor unplugged until a tablet connects (no UAC for the app).
  DetailPrint "Registrando o serviço do TabDisplay..."
  !insertmacro TD_CHECK 'sc create TabDisplay binPath= "\"$INSTDIR\tabdisplay.exe\" --service" start= auto DisplayName= "TabDisplay"' 'Criar serviço TabDisplay'
  !insertmacro TD_CHECK 'sc config TabDisplay binPath= "\"$INSTDIR\tabdisplay.exe\" --service" start= auto' 'Configurar serviço TabDisplay'
  !insertmacro TD_CHECK 'sc description TabDisplay "Liga o monitor virtual do TabDisplay só enquanto um tablet está conectado."' 'Descrever serviço TabDisplay'
  !insertmacro TD_CHECK 'sc failure TabDisplay reset= 86400 actions= restart/5000/restart/5000/restart/5000' 'Configurar recuperação do serviço TabDisplay'
  !insertmacro TD_CHECK 'sc start TabDisplay' 'Iniciar serviço TabDisplay'
  !insertmacro TD_WAIT_RUNNING
  DetailPrint "Serviço: código $0 (0 = ok)"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro TD_CHECK_OPTIONAL_SERVICE 'sc stop TabDisplay' 'Parar o serviço TabDisplay'
  !insertmacro TD_WAIT_STOPPED
  !insertmacro TD_CHECK_OPTIONAL_SERVICE 'sc delete TabDisplay' 'Remover serviço TabDisplay'
  !insertmacro TD_CHECK '"$INSTDIR\tabdisplay.exe" --uninstall-driver' 'Remover somente o driver pertencente ao TabDisplay'
  !insertmacro TD_CHECK 'netsh advfirewall firewall delete rule name="TabDisplay"' 'Remover regra do firewall'
!macroend
