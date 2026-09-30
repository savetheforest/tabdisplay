# T30 — Revalidar viabilidade do Linux e dividir a implementação

Prioridade: P3. Tipo: Spike opcional. Status: Concluída.
Dependências: T01, T02, T05, T06, T08, T17, T23.

## Contexto

docs/linux.md é uma análise anterior; lib.rs só seleciona sys para Windows/Mac. As estimativas daquele documento não são prazo validado. Linux depende de compositor, portal, driver e permissões.

## Investigação e etapas propostas

1. Escolher uma distribuição/compositor/versionamento de referência. Registrar X11/Wayland e GPU; não prometer “Linux universal”.
2. Revisar a interface sys após T15/T16, evitando impor tipos Windows ao backend.
3. Prototipar primeiro Mirror em Wayland por portal + PipeWire; aprovação do usuário faz parte do fluxo. Validar revogação, cancelamento e token de restauração.
4. Decidir backend X11 separadamente; não usar APIs que contornem permissões de Wayland.
5. Para input, avaliar portal RemoteDesktop/libei antes de exigir uinput. Se uinput for necessário, projetar privilégio mínimo; não chmod 666 nem executar app inteiro como root.
6. Avaliar loopback PipeWire/PulseAudio, dispositivo padrão e ausência de áudio.
7. Iniciar com fallback CPU para prova funcional e medir; VAAPI/NVENC só com detecção/benchmark.
8. Tratar Extend como outro conjunto de tarefas por compositor. Verificar APIs atuais de GNOME/KDE/wlroots; xrandr --setmonitor sozinho não garante monitor virtual capturável.
9. Avaliar Tauri/WebKitGTK, distribuição AppImage/deb/rpm, ADB, regras USB e atualização/assinatura.
10. Produzir tarefas filhas: L1 Mirror; L2 input/áudio; L3 empacotamento/teste; L4 Extend por compositor; L5 encoder GPU. Cada uma deve especificar versão/plataforma e aceite próprio.

## Critérios de aceite

- [ ] Decisão identifica plataformas suportáveis e alternativas sem root.
- [ ] Contratos reutilizados e adaptações necessárias estão listados.
- [ ] Protótipo, se feito, tem teste real em ambiente Linux designado.
- [ ] Nenhuma promessa de Extend multiplataforma baseada apenas em API citada.
- [ ] docs/linux.md recebe fatos novos com fontes oficiais e limitações.

Sem ambiente Linux, concluir somente investigação documental com validação física pendente. Não instalar distro/driver nem alterar boot da máquina do usuário.

Rollback mantém seleção de plataforma existente e remove/desativa protótipo isolado; não fingir suporte criando módulos vazios que “compilam”.
