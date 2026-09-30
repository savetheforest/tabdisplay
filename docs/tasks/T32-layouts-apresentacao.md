# T32 — Salvar layouts e oferecer um modo apresentação

Prioridade: P3. Tipo: Opcional. Status: Em validação.
Dependências: T10, T18, T19, T22.

## Objetivo e limites

Salvar preferências de posição/monitor/perfil por tablet e oferecer apresentação sem controle acidental. Não mover janelas de outros aplicativos automaticamente na primeira versão.

## Implementação

Núcleo inicial implementado em `desktop/src-tauri/src/layouts.rs`:

- schema `1` com preset por `device_id`, modo, posição, monitor, perfil,
  áudio, input, modo de toque e opções de apresentação;
- documento com até 16 presets, IDs únicos e limites de tamanho;
- leitura do formato legado de preset único e migração sem descartar os campos
  conhecidos;
- persistência via substituição atômica existente de configurações;
- plano de aplicação que retorna ajustes explícitos para licença ausente e
  monitor removido, sem trocar silenciosamente para o monitor principal;
- sessão de apresentação que desabilita input por padrão e restaura o preset
  anterior ao sair.

Integração local adicionada:

- `layout_presets` lê o documento da configuração do app;
- `save_layout_preset` salva explicitamente o estado atual da sessão escolhida;
- `plan_layout_preset` verifica o preset contra licença e monitores presentes e
  retorna os ajustes sem aplicá-los;
- `apply_layout_preset` aplica somente após confirmação, quando existe um único
  tablet conectado e o plano não possui ajustes; preserva campos de vídeo que
  não pertencem ao preset;
- a UI de Avançado permite selecionar a sessão ativa, salvar o layout e
  verificar o plano. O `device_id` é usado apenas para separar presets no
  estado local da UI.

O módulo não aplica topologia automaticamente nem move janelas de outro
aplicativo. A UI exige a verificação antes de aplicar; com dois tablets ou
qualquer ajuste pendente, a aplicação é recusada explicitamente.

1. Definir schema versionado de preset: device autenticado, modo, posição, monitor preferido, qualidade, áudio e input.
2. Separar configuração desejada de hardware disponível. Monitor ausente pede alternativa, não espelhamento silencioso do principal.
3. Aplicar preset de forma transacional com resultado por componente; preservar preset original se houver redução por capacidade/licença.
4. Modo apresentação desabilita input remoto conforme escolha, permite ocultar menu/stats e mantém uma forma acessível de sair.
5. Definir interação de manter tela ligada, orientação e áudio com T10. Restaurar preferências anteriores ao sair do modo.
6. Dois tablets podem ter presets distintos sem mudar estado global; conflitos de topologia precisam de mensagem.
7. Perfil por aplicativo só entra em fase posterior com detecção de foco e decisão de privacidade; não ler título de janela nem enviar lista de apps por padrão.
8. Não prometer ocultação de notificações/conteúdo do PC no espelhamento inteiro; captura exclusiva de janela seria outra tarefa.

## Aceite e testes

- [x] O núcleo salva, recarrega e migra presets sem perder preferências conhecidas.
- [x] O núcleo produz ajuste explícito para licença/monitor incompatível.
- [x] O núcleo de apresentação desabilita input por padrão e restaura o estado anterior.
- [x] UI lista a sessão ativa, salva o preset por tablet e mostra o plano de aplicação.
- [x] Aplicação confirmada em sessão única sem ajuste pendente preserva campos não pertencentes ao preset.
- [ ] UI/driver aplicam presets independentes com dois tablets e apresentam escolha de alternativa.
- [ ] Apresentação integrada ao Android/desktop permite sair de forma acessível.
- [ ] Desconectar restaura o estado temporário definido, sem alterar outras sessões.

Testar monitor removido, dois tablets, resolução indisponível, licença ausente e crash no meio da aplicação. Usar storage temporário.

Rollback conserva presets como dados versionados, sem aplicá-los automaticamente por binário que não os entende.
