# T16 — Pipeline e compatibilidade macOS

## Identificação

- ID e título: T16 — Otimizar o caminho Mac preservando ownership e compatibilidade
- Status: Em validação
- Base: `d823e5b` + T01–T15 parciais + working tree de 29/09/2026
- Dependências verificadas: T03, T04, T05 e T07 implementados localmente
- Plataforma disponível nesta execução: Windows; nenhum macOS/Apple Silicon disponível

## Problema e resultado

O caminho macOS já usava ScreenCaptureKit para dimensionar a captura, mas copiava
pixels para um `Vec` compartilhado e copiava novamente para um `CVPixelBuffer`.
Também não usava o FPS efetivo para o intervalo mínimo da captura e ignorava
alguns erros críticos do VideoToolbox.

Foram aplicadas correções de segurança e compatibilidade no caminho existente:

- `SCStreamConfiguration` recebe o FPS negociado, limitado a 1–240, em vez de
  sempre pedir 120 Hz.
- A fila é explicitamente limitada pelo `queue_depth(3)` existente; o frame
  mais recente é reutilizado e o buffer compartilhado é zerado antes de copiar
  uma captura menor, evitando restos de uma imagem anterior.
- Falhas de mutex/condvar no callback e no consumidor viram ausência de frame,
  não panic.
- O encoder rejeita frame BGRA curto, falha de lock/null base/stride inválido e
  erro de `CompleteFrames`.
- Resultados de propriedades essenciais, preparação do encoder e parâmetros de
  VideoToolbox agora são propagados como erro explícito.
- O callback C não faz `unwrap`, verifica sample/data buffer/format description
  nulos e sai com segurança quando o estado não pode ser lido.
- O caminho AVCC continua convertendo cada NAL para uma access unit Annex-B e
  inclui SPS/PPS antes de keyframes; não foi substituído por uma pipeline
  assíncrona sem benchmark.
- O parser AVCC foi extraído para `src/avcc.rs`, compartilhado pelo encoder macOS
  e testado no host Windows com NALs múltiplos, tail truncado e NAL vazio.

Não foi feita uma tentativa de reter o `CVPixelBuffer` do ScreenCaptureKit
diretamente no VideoToolbox: isso requer verificar as chaves de pool,
retain/release e a vida útil dos callbacks em macOS real. A cópia CPU e a
barreira `CompleteFrames` permanecem o fallback auditável.

## Ownership e limites

- O `Vec<Mutex>` usado como `refcon` fica dentro de `HwEncoder` até depois de
  `VTCompressionSessionInvalidate` e `CFRelease`; o callback só acessa esse
  objeto enquanto a sessão está viva.
- Cada pixel buffer criado pelo pool é liberado após a conclusão do frame.
- A captura mantém apenas o último frame e `queue_depth(3)`; não há fila sem
  limite nem retenção de `CMSampleBuffer`.
- PTS e duração usam contador monotônico de frames no FPS efetivo.

## Validação

| Verificação/cenário | Ambiente e comando | Resultado observado | Artefato |
|---|---|---|---|
| Build/testes desktop Windows | `cargo test --locked` | Passou: 45 unitários, 1 ignorado, 1 integração; não compila o módulo `cfg(macos)` | suíte Cargo |
| AVCC com NALs múltiplos, NAL vazio e prefixo truncado | testes unitários `avcc::tests` | Passou no host Windows no módulo puro compartilhado; o encoder macOS usa a mesma função | `src/avcc.rs`, `mac/encode.rs` |
| macOS Apple Silicon, ScreenCaptureKit e VideoToolbox | Mac real | Não executado; sem host/macOS nesta sessão | pendente |
| permissões, sleep/wake, Retina, display removido e duas sessões | Mac real | Não executado | pendente |
| benchmark de cópias, encode, FPS, consumo e modo assíncrono | Mac release, 30 s + 60 s × 3 | Não medido; não há hardware macOS disponível | pendente |

Não há alegação de compatibilidade com uma versão específica de macOS ou de
encoder hardware efetivo. Compilar no Windows não valida APIs, ABI ou permissões
do macOS.

## Operação e reversão

O comportamento público permanece igual. Para reverter, restaurar apenas as
guardas de erro e o intervalo anterior no código macOS, preservando o limite da
fila e o fallback. Não há dados persistidos nem migração.

Próximo passo exato: executar em Apple Silicon um benchmark de captura/cópia/
VideoToolbox e um teste de encerramento/suspensão; somente se o resultado
justificar, criar uma etapa isolada para pool de `CVPixelBuffer` retido e callback
assíncrono.
