# Relatório T32 — Layouts e apresentação

Data: 29/09/2026  
Status: Em validação

Foi adicionado `desktop/src-tauri/src/layouts.rs` com schema versionado `1` para presets por tablet: device autenticado, modo, posição, monitor preferido, perfil, áudio, input, touch mode e opções de apresentação. O documento limita a 16 tablets, rejeita IDs/monitores excessivos, impede duplicidade e usa a substituição atômica já existente em `settings::persist`.

O parser migra o formato legado de preset único para o documento versionado sem perder os campos conhecidos. `plan_apply` mantém o preset pedido e retorna ajustes explícitos quando a licença não permite Extend ou quando o monitor escolhido desapareceu; não há fallback silencioso para o monitor principal. `PresentationSession` desliga input por padrão e restaura o preset anterior ao sair.

Os seis testes unitários do módulo passaram: persistência/reload de dois tablets, migração, IDs/duplicidade, ajustes de licença/monitor, restauração do modo apresentação e aplicação que preserva campos não pertencentes ao preset. `cargo test --locked` específico de T32 concluiu com 6 aprovados.

O release foi recompilado e reiniciado sem limpar dados. Para a regressão física, o Wi‑Fi foi temporariamente desligado, o tablet foi reconectado pelo item explícito “Conexão local pelo ADB” e o PC confirmou `127.0.0.1:7070 ↔ 127.0.0.1:49385`; o `SurfaceView` permaneceu ativo em `46 fps · 5 ms · 8,6 Mbps`. O Wi‑Fi foi restaurado depois e os arquivos XML temporários de diagnóstico foram removidos.

O núcleo ainda não está ligado ao ciclo completo de aplicação independente por sessão. A aplicação agora é permitida somente após confirmação, com um único tablet e nenhum ajuste pendente; com dois tablets, licença ausente ou monitor removido, ela é recusada explicitamente. Apresentação deverá manter uma saída acessível, coordenar tela ligada/orientação/áudio com T10 e restaurar preferências ao desconectar. Dois tablets permanecem independentes.

Nesta continuação, a integração local foi acrescentada: `SessionInfo` expõe o identificador autenticado somente para a UI local; `layout_presets` carrega o documento; `save_layout_preset` grava o estado atual da sessão selecionada; `plan_layout_preset` retorna ajustes; e `apply_layout_preset` aplica somente o plano sem ajustes em sessão única. A página Avançado ganhou seleção da sessão, “Salvar atual”, “Verificar” e “Aplicar”. `node --check desktop/src/main.js`, 6 testes específicos de T32 e o rebuild release passaram.

Após essa aplicação segura, o release foi reiniciado e a sessão USB foi repetida com Wi‑Fi temporariamente desligado: `127.0.0.1:7070 ↔ 127.0.0.1:52325`, `SurfaceView` ativo; Wi‑Fi restaurado, dados preservados e XML temporário removido. A automação de clique da janela desktop continua indisponível, portanto a ação “Aplicar” não foi contada como teste físico.

O release foi reiniciado e a sessão física do Redmi Pad 2 permaneceu ativa; após desligar temporariamente o Wi‑Fi, o item explícito “Conexão local pelo ADB” confirmou `127.0.0.1:7070 ↔ 127.0.0.1:55488` e `SurfaceView` em `47 fps · 10 ms · 7,8 Mbps`; o Wi‑Fi foi restaurado sem perder a sessão. A automação nativa de janela do PC não estava disponível nesta execução, então o clique dos controles da UI não foi contado como aprovado físico. A lógica de comandos permanece coberta pelo compilador/testes do núcleo.

Pendentes: UX de salvar/aplicar, restauração após crash, resolução indisponível, desconexão durante aplicação, dois tablets físicos e integração Android/desktop.
