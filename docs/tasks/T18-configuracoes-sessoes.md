# T18 — Isolar configurações por tablet e sincronizar o estado da UI

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T01, T02, T11.

## Contexto

PROFILE vindo de um tablet altera Settings global, causando rebuild em todos. status mostra principalmente sessions.first. settings::set incrementa VERSION até para onboarded/áudio/toque. A UI desktop sincroniza profile, mas pode ficar com mode antigo após ação na bandeja e regravá-lo depois.

## Implementação

1. Definir camadas: preferências globais, default para novos devices, preferências por device_id autenticado e estado efetivo da sessão.
2. Separar resolução/FPS/bitrate/encoder/posição/monitor/input/áudio de licença, autostart, onboarding e preferências de janela.
3. Definir quais controles no PC afetam tablet selecionado ou default. Exibir alvo de forma clara; não mudar dois tablets silenciosamente.
4. PROFILE recebido deve alterar somente sessão/dispositivo autorizado conforme contrato. Nome de device não é chave de identidade.
5. Classificar mudanças: ao vivo (mute/input), reconfiguração do encoder, resize do stream ou alteração do monitor. Não reconstruir vídeo para concluir onboarding.
6. Emitir estado/versionamento para a UI inteira; não sincronizar só profile. Tratar alterações concorrentes por patch/revisão, evitando sobrescrever configuração recente com objeto antigo.
7. Mostrar estatísticas/status e ação de desconectar por sessão, preservando resumo global útil.
8. Validar valores no backend e devolver valores efetivos/erro ao frontend; persistência em arquivo temporário + substituição atômica apropriada, com falhas reportadas.
9. Migrar settings.json v0.2.0 para defaults sem inventar preferência individual para devices desconhecidos. Preservar backup limitado e não repetir migração.
10. Manter licença verificada no backend para todos os caminhos, inclusive bandeja e mensagens futuras.
11. Atualizar lista de monitores ao conectar/desconectar displays, não apenas no load inicial.

## Critérios de aceite

- [ ] Tablet A muda perfil sem reconstruir B.
- [ ] Bandeja/desktop/tablet concordam com estado real e não se sobrescrevem por polling.
- [ ] Áudio, toque e onboarding não reiniciam captura quando desnecessário.
- [ ] Estatísticas identificam claramente a sessão.
- [ ] Falha de disco é visível, sem alegar preferência salva.
- [ ] Migração preserva preferências e bloqueio de licença.

## Validação e rollback

Dois tablets com resoluções opostas, mudança concorrente pela bandeja e UI, valores fora de faixa, disco sem escrita e JSON truncado. Não usar o arquivo de configuração pessoal em testes.

Rollback lê formato anterior ou converte com backup explícito; não sobrescrever configuração nova com defaults só porque o binário antigo não entende o schema.
