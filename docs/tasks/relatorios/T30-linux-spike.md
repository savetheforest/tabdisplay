# Relatório T30 — Linux

Data: 29/09/2026  
Status: Concluída como spike

Decisão: não anunciar Linux nem criar módulos vazios. `docs/linux.md` continua uma análise preliminar; sem host Linux não há validação de portal PipeWire, X11, Wayland, RemoteDesktop/libei, áudio, Tauri/WebKitGTK ou empacotamento.

Plano condicionado a ambiente designado: L1 Mirror Wayland por portal, L2 input/áudio com privilégio mínimo, L3 pacote/assinatura/testes, L4 Extend por compositor e L5 encoder GPU. Extend não será inferido de `xrandr --setmonitor`, e uinput não receberá chmod amplo nem exigirá rodar o app como root.
