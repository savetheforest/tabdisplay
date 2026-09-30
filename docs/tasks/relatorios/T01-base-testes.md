# T01 — Base de testes e fixtures

## Identificação

- ID e título: T01 — Construir uma base de testes útil para os dois clientes
- Status: Concluída
- Commit base e revisão testada: `d823e5b` + working tree desta entrega em 29/09/2026
- Plataformas/versões: Windows; Rust/cargo 1.98.0; Java 17.0.20.1; Gradle 9.7.1; Android compile/target SDK 36
- Dependências verificadas: nenhuma; T01 é a fundação do backlog

## Problema e resultado

- Cenário que disparava o problema: o Android não tinha testes JVM (`testDebugUnitTest` era `NO-SOURCE`) e o contrato binário era exercitado apenas por testes pontuais no desktop.
- Evidência antes: não havia diretório `android/app/src/test`, codec de framing puro ou fixtures compartilhadas; `cargo test --locked` tinha 21 testes, com o teste de áudio físico ignorado.
- Comportamento depois: Rust e Kotlin consomem os mesmos fixtures v3; o Android testa framing, truncamento, limite de mensagem, INPUT e SCROLL em JVM; o `Stream` usa o codec puro para ler/escrever as mensagens; o parser INPUT rejeita enum, bits reservados, números não finitos e coordenadas fora de faixa.
- Arquivos e símbolos alterados: `test-fixtures/v3/*`; `Protocol.kt`; `InputFrame`/adaptador em `Input.kt`; `Stream.kt`; configuração/testes JVM Android; helpers de fixture e testes em `server.rs`, `input.rs` e `lib.rs`; `README.md` de fixtures.

## Decisões

- Contratos de API/protocolo e compatibilidade: o cabeçalho continua `[type:u8][len:u32 BE]`, limite de 16 MiB e payloads v3; o framing foi centralizado apenas no cliente Android, sem mudança de versão ou wire format.
- Ownership, limites, timeouts e cancelamento: `Protocol.Reader` aloca somente após validar o tamanho; EOF/truncamento falham explicitamente; `Stream` mantém o executor e o fechamento existentes.
- Dados persistidos e migração: nenhum.
- Alternativa descartada e motivo, se relevante: não foi criado um segundo conjunto de fixtures dentro do Android; os recursos vêm de `test-fixtures/v3` para evitar divergência Rust/Kotlin. MotionEvent não foi levado para o teste JVM; a adaptação Android chama o `InputFrame` puro.

## Validação

| Verificação/cenário | Ambiente e comando | Resultado observado | Artefato |
|---|---|---|---|
| Baseline desktop | Windows; `cargo test --locked` antes da mudança | Passou: 21 testes, 1 ignorado | saída do comando; baseline em `PROGRESSO.md` |
| Fixtures/framing/INPUT Rust | Windows; `cargo test --locked` após a mudança | Passou: 22 testes unitários, 1 ignorado, e 1 teste de integração | `desktop/src-tauri/src/input.rs`, `server.rs` |
| Checagem de compilação desktop | Windows; `cargo check --locked` | Passou; somente warning preexistente de `find_device` | saída do comando |
| Framing/INPUT/SCROLL Kotlin | Windows; `android\.\gradlew.bat testDebugUnitTest` | Passou: 3 testes reais; não ficou `NO-SOURCE` | `android/app/src/test/java/com/tabdisplay/ProtocolFixturesTest.kt` |
| APK debug | Windows; `android\.\gradlew.bat assembleDebug` | Passou; APK em `android/app/build/outputs/apk/debug/app-debug.apk` | APK de desenvolvimento |
| Lint Android | Windows; `android\.\gradlew.bat lintDebug` | Não passou por `android/local.properties` gerado localmente com caminho Windows não escapado; o lint reporta 1 erro `PropertyEscape` e 13 warnings. Não é erro nos arquivos da T01. | `android/app/build/reports/lint-results-debug.html` |
| Smoke de socket | Windows; teste Rust existente `tls::tests::handshake_and_duplex_traffic` | Passou em loopback e porta efêmera; não inicia Tauri nem usa hardware | `desktop/src-tauri/src/tls.rs` |

- Comparação de desempenho, quando aplicável: não aplicável à base de testes; nenhum FPS/latência foi inventado.
- Testes físicos não executados: tablet Android, ADB/USB e decoder físico; `adb` não está no PATH.
- Critérios de aceite ainda pendentes: nenhum critério obrigatório da T01. O lint exige correção do arquivo de ambiente local ou uma política de lint separada; não foi alterado para não incluir configuração de máquina no repositório.

## Operação e reversão

- Como ativar/desativar a mudança: os testes rodam normalmente com os comandos acima; não há flag de produto.
- Como voltar sem perder configurações/pareamentos: remover os adaptadores/testes adicionados e a dependência JUnit; fixtures são somente de teste e não participam de dados do usuário.
- Limitações conhecidas: testes Kotlin são JVM e não substituem MediaCodec, Surface ou lifecycle instrumentado; fixtures não provam decoder, transporte físico ou performance.
- Próximo passo exato: atualizar o checkpoint e iniciar T02 somente após ler `docs/tasks/T02-protocolo.md` e revalidar framing/limites no código atual.
