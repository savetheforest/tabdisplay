# T02 — Contrato de protocolo e validação

## Identificação

- ID e título: T02 — Formalizar o protocolo e validar entradas antes de usar recursos
- Status: Concluída
- Commit base e revisão testada: `d823e5b` + T01 + T02 no working tree em 29/09/2026
- Plataformas/versões: Windows; Rust/cargo 1.98.0; Java 17.0.20.1; Gradle 9.7.1; Android SDK 36
- Dependências verificadas: T01 concluída; fixtures Rust/Kotlin disponíveis

## Problema e resultado

- Cenário que disparava o problema: `read_msg` aplicava apenas o teto global de 16 MiB; HELLO/RESIZE/Settings podiam carregar dimensões, strings, FPS, bitrate e conversões sem limites próprios; INPUT não rejeitava IDs duplicados.
- Evidência antes: leitura alocava o tamanho do cabeçalho antes de conhecer o tipo; o servidor fazia `as u32` em RESIZE; `Settings::set` persistia valores sem validação; mensagens conhecidas fora do fluxo eram ignoradas.
- Comportamento depois: limites por tipo são aplicados antes da alocação; HELLO exige v3, dimensões/DPI/identidade válidas; RESIZE, PONG, STATS, Settings e INPUT têm validação limitada; Android e Rust rejeitam tipo desconhecido, framing truncado e INPUT inválido; estados, limites e compatibilidade v3 estão documentados em `PROTOCOL.md`.
- Arquivos e símbolos alterados: `PROTOCOL.md`, `server.rs` (`read_msg`, `validate_hello`, `parse_dimensions`, limites), `input.rs`, `settings.rs`, `Protocol.kt`, `Stream.kt`, `MainActivity.kt`, fixtures/testes e este relatório.

## Decisões

- Contratos de API/protocolo e compatibilidade: permanece v3, sem downgrade para plaintext; mensagens JSON de controle têm 64 KiB, VIDEO 8 MiB, AUDIO 256 KiB e INPUT 4591 bytes; `config_id` foi adotado depois como extensão opcional de CONFIG/STATS sem alterar o VIDEO binário; pedido de IDR e PTS continuam propostas.
- Ownership, limites, timeouts e cancelamento: o framing valida o comprimento antes de criar o payload; o parser de entrada usa limite fixo e IDs únicos; a política de deadline total de HELLO/pareamento fica especificada para T06, sem simular um timeout parcial nesta tarefa.
- Dados persistidos e migração: Settings inválido vindo do disco cai nos defaults; `set` rejeita valores inválidos sem gravar; formato válido e pareamentos existentes não mudam.
- Alternativa descartada e motivo, se relevante: não foi introduzida uma nova versão para os limites; v3 já tem erro determinístico para tipos desconhecidos e campos opcionais continuam sem autorização implícita.

## Validação

| Verificação/cenário | Ambiente e comando | Resultado observado | Artefato |
|---|---|---|---|
| Corpus/headers Rust | Windows; `cargo test --locked` | Passou: 23 testes unitários, 1 ignorado, e 1 integração; cobre truncamento, JSON inválido, limite por tipo, dimensões e HELLO fixture | `desktop/src-tauri/src/server.rs` |
| Compilação desktop | Windows; `cargo check --locked` | Passou; permanece apenas warning preexistente de `find_device` | saída do comando |
| Framing/limites/dimensões Android | Windows; `android\.\gradlew.bat testDebugUnitTest assembleDebug` | Passou: 3 testes JVM, incluindo tipo desconhecido, limite INPUT, truncamento e dimensões | `android/app/src/test/java/com/tabdisplay/ProtocolFixturesTest.kt` |
| Compatibilidade dos fixtures | Rust/Kotlin, recursos `test-fixtures/v3` | HELLO/CONFIG, INPUT e SCROLL continuam byte-a-byte concordantes | `test-fixtures/v3/*` |
| Lint Android | Windows; `android\.\gradlew.bat lintDebug` | Não passou por `android/local.properties` gerado localmente com caminho Windows não escapado; 1 erro `PropertyEscape` e 13 warnings, fora dos arquivos T02 | `android/app/build/reports/lint-results-debug.html` |

- Comparação de desempenho, quando aplicável: não aplicável; nenhuma captura/encoder foi iniciado pelos testes.
- Testes físicos não executados: tablet Android, ADB/USB, MediaCodec e drivers.
- Critérios de aceite ainda pendentes: nenhum obrigatório; o prazo efetivo de I/O e pareamento é explicitamente dependência de T06.

## Operação e reversão

- Como ativar/desativar a mudança: automática no framing e nos parsers; sem flag de produto.
- Como voltar sem perder configurações/pareamentos: reverter somente as validações/adaptadores; Settings rejeitado nunca substitui dados válidos, e o formato persistido permanece o mesmo.
- Limitações conhecidas: a tabela formaliza a política, mas T06 ainda precisa garantir deadlines de socket contra peer lento; testes JVM não substituem lifecycle/MediaCodec físico.
- Próximo passo exato: ler `docs/tasks/T06-tls-concorrencia.md`, revalidar `tls.rs` e `server.rs` contra peer que não lê ou envia HELLO lentamente, e implementar cancelamento sem mutex durante I/O.
