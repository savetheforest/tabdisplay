# T04 — Temporização e resolução no espelhamento

## Identificação

- ID e título: T04 — Corrigir temporização e aplicar a resolução escolhida ao espelhamento
- Status: Em validação
- Commit base e revisão testada: `d823e5b` + T01/T02/T03 + working tree de 29/09/2026
- Plataformas/versões: Windows; Rust/cargo 1.98.0
- Dependências verificadas: T01 concluída; T03 implementada e em validação física

## Problema e resultado

- Cenário que disparava o problema: `last` era atualizado depois de encode e escrita TLS, somando o custo de trabalho ao intervalo; no Mirror, `open_capture` recebia o máximo do tablet, não o tamanho efetivo escolhido.
- Evidência antes: cadeia `cap.next → encode → write_msg → last=Instant::now()` e `open_capture(device, tablet)` confirmadas em `server.rs`.
- Comportamento depois: `next_deadline_after` mantém o relógio monotônico, avança deadlines vencidos sem rajada e usa timeout de captura limitado; a captura recebe `(w,h)` efetivos e escala mantendo proporção, sem alterar o monitor físico ou o rect de input.
- Arquivos e símbolos alterados: `desktop/src-tauri/src/server.rs` e este relatório.

## Decisões

- Contratos de API/protocolo e compatibilidade: CONFIG continua reportando tamanho e FPS efetivos; o monitor fonte e suas coordenadas não são redimensionados por causa do stream.
- Ownership, limites, timeouts e cancelamento: o scheduler não cria fila de frames; apenas o buffer bruto mais recente e o estado `pending` existem. Quando a captura está ociosa, o timeout é o intervalo limitado a 50 ms, evitando busy loop.
- Dados persistidos e migração: nenhum.
- Alternativa descartada e motivo, se relevante: não foi usado `sleep(interval)` após encode; isso voltaria a somar o custo de captura/encode/write e reduziria FPS.

## Validação

| Verificação/cenário | Ambiente e comando | Resultado observado | Artefato |
|---|---|---|---|
| Custo abaixo do orçamento e salto de deadlines vencidos | Windows; `cargo test --locked` | Passou: teste monotônico cobre 8 ms sem atraso adicional, 25 ms e 80 ms sem slots acumulados; suíte ficou com 25 testes unitários, 1 ignorado, e 1 integração | `desktop/src-tauri/src/server.rs` |
| Build/checagem desktop | Windows; `cargo check --locked` (executado em T02/T03; alteração é Rust compilável na suíte) | Passou na revisão anterior; `cargo test` recompilou o crate e passou | saída dos comandos |
| Mirror/Performance/Custom em monitor real | Release workspace + Redmi Pad 2 USB | Sessão física ativa com SurfaceView; PC reportou 1152x640, 60 fps, 10 Mbps, encoder GPU; comparação 4K/Mirror ainda pendente | log do processo e UI ADB |
| Cursor/tecla isolados e quatro cantos | Não executado | pendente: exige cenário seguro dedicado; toque/arraste central foi exercitado sem queda de sessão | validação física |

- Comparação de desempenho, quando aplicável: não medir ainda; T05 definirá instrumentação e benchmark comparável.
- Testes físicos executados: APK release no Redmi Pad 2, pareamento/reconexão USB, Surface, vídeo, toque/arraste central e uma retomada após Home. Permanecem pendentes 4K/Mirror, rotação, cantos/letterbox e benchmark comparável.
- Critérios de aceite parcialmente atendidos: houve evidência de CONFIG/stream efetivos e estabilidade visual; update isolado, mapeamento completo e medição externa de latência ainda exigem cenário dedicado.

## Operação e reversão

- Como ativar/desativar a mudança: automática em `stream_once`; sem flag.
- Como voltar sem perder configurações/pareamentos: reverter `next_deadline_after`/uso de `(w,h)` em `open_capture`; nenhum estado persistido muda.
- Limitações conhecidas: o FLUSH de 250 ms permanece provisório e precisa de validação em decoders reais; não há benchmark de FPS nesta tarefa.
- Próximo passo exato: ler `docs/tasks/T05-metricas-benchmark.md` e adicionar métricas separadas de captura, encode, envio, decode e render observado sem chamar RTT de latência visual.
