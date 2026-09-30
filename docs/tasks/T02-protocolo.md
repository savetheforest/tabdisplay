# T02 — Formalizar o protocolo e validar entradas antes de usar recursos

Prioridade: P0. Tipo: Core. Status: Concluída.
Dependências: T01. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Problema e objetivo

O limite global de 16 MiB não é suficiente: dimensões, FPS, strings e mensagens fora de ordem precisam de validação própria. Novos recursos exigirão capabilities sem quebrar clientes antigos.

## Onde investigar

PROTOCOL.md; server.rs: Hello/read_msg/handle/RESIZE/preset; input.rs; settings.rs; Stream.kt: run/resize; MainActivity.kt: HELLO. Os caminhos completos estão em EVIDENCIAS.md.

## Implementação

1. Especificar estados: TLS, aguardando HELLO, pareamento, configurando, streaming, pausado e encerrando. Definir tipos permitidos por estado e reação a duplicatas.
2. Definir limites por tipo antes de alocar: controle JSON pequeno, IDs/nomes limitados, vídeo com teto explícito e INPUT no máximo suportado. Manter o teto global como segunda barreira.
3. Validar dimensões mínimas/máximas, produto w*h, alinhamento, FPS positivo, bitrate e conversões u64→u32/usize. Rejeitar overflow, NaN/Inf, IDs de contato duplicados e coordenadas/pressões inválidas.
4. Validar configurações vindas da UI/disco tão rigorosamente quanto as de rede; regras compartilhadas com T18, sem confiar no slider.
5. Especificar prazo total de HELLO/pareamento e rate limits; T06 executará o controle de I/O.
6. Definir envelope de capacidades e configuração efetiva, com versão de schema, valores opcionais e comportamento quando ausentes. A lista de modos pode ser limitada; não enviar inventário ilimitado.
7. Propor IDs e contratos de pausa/resume, pedido de IDR, estatísticas e erros estruturados. Identificar cada extensão como proposta até implementada; não enviar mensagem não negociada.
8. Decidir compatibilidade: extensões opcionais em v3 com defaults e fixtures, ou nova versão com recusa clara. Não aumentar a versão várias vezes por tarefas concorrentes.
9. Definir generation/config_id para associar frames a configuração, unidade de PTS e sequência quando houver metadados. Mudança do VIDEO binário precisa negociação/versão explícita.
10. Atualizar PROTOCOL.md e fixtures bidirecionais. Remover comentários “v2”/plaintext obsoletos apenas quando o contrato real estiver confirmado.

## Cuidados e fora de escopo

- Não relaxar autenticação por causa de incompatibilidade.
- Não ignorar mensagem desconhecida sem limite: documentar se é extensão negociada, erro recuperável ou encerramento.
- Erros ao usuário não devem revelar segredos ou ecoar payload malicioso sem truncamento.
- Capability não é autorização: permissão para input/arquivos/clipboard é verificada independentemente.
- Esta tarefa define extensões; não implementa todos os recursos opcionais.

## Critérios de aceite

- [ ] Entradas inválidas não provocam panic, tamanho zero de frame, overflow nem alocação arbitrária.
- [ ] Existe tabela de mensagens por estado, limites, unidades e política de erro.
- [ ] Versões antiga/nova têm resultado determinístico nas duas direções.
- [ ] HELLO incompatível produz mensagem útil, sem iniciar captura/driver.
- [ ] Rust/Kotlin usam fixtures concordantes; extensões propostas são distinguíveis das disponíveis.

## Validação e rollback

Usar corpus válido e malformado, headers fragmentados, strings longas, dimensão zero/gigante e mensagens repetidas fora de ordem. Testar cliente legado de fixture e mensagens extras opcionais.

Preservar leitura do formato persistido antigo. Rollback do protocolo precisa manter cliente/servidor compatíveis ou bloquear claramente a combinação; nunca retornar a plaintext.
