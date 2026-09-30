# T06 — TLS, cancelamento e conexões lentas

## Identificação

- ID e título: T06 — Evitar bloqueios em TLS e encerrar conexões de forma limitada
- Status: Em validação
- Commit base e revisão testada: `d823e5b` + T01/T02 + working tree de 29/09/2026
- Plataformas/versões: Windows; Rust/cargo 1.98.0; Java 17.0.20.1; Gradle 9.7.1
- Dependências verificadas: T01 e T02 concluídas

## Problema e resultado

- Cenário que disparava o problema: `Conn::shutdown` adquiria o mutex TLS e podia bloquear em `write_tls`; `Conn::write` não tinha timeout de escrita; HELLO e PAIR usavam `read_exact` sem deadline total; `register` fechava a sessão antiga enquanto segurava `SESSIONS`; a fila de envio Android era ilimitada e engolia falhas.
- Evidência antes: confirmada por inspeção dos caminhos `tls.rs`, `server.rs` e `Stream.kt`; o teste duplex existente cobria records grandes, mas não peer que para de ler.
- Comportamento depois: socket tem timeout de escrita de 5 s; shutdown faz TCP shutdown primeiro e só tenta `close_notify` com `try_lock`; HELLO tem deadline total de 10 s, pareamento de 180 s, e heartbeat encerra após 15 s sem PONG; sessão antiga é retirada do registro antes do shutdown; envio Android usa fila limitada a 64 tarefas e fecha a sessão uma única vez em erro/rejeição.
- Arquivos e símbolos alterados: `desktop/src-tauri/src/tls.rs`, `server.rs`, `android/app/src/main/java/com/tabdisplay/Stream.kt`.

## Decisões

- Contratos de API/protocolo e compatibilidade: TLS 1.3 e framing v3 permanecem; nenhum downgrade ou troca para UDP foi feita.
- Ownership, limites, timeouts e cancelamento: o mutex protege o estado `ServerConnection`; I/O de socket é limitado por timeout, e cancelamento usa shutdown TCP sem esperar o mutex. Deadlines de mensagens são totais, não renovados indefinidamente por byte recebido.
- Dados persistidos e migração: nenhum.
- Alternativa descartada e motivo, se relevante: não foram permitidos dois escritores concorrentes no mesmo `ServerConnection`; records continuam serializados pelo mutex.

## Validação

| Verificação/cenário | Ambiente e comando | Resultado observado | Artefato |
|---|---|---|---|
| TLS duplex e records fragmentados/grandes | Windows; `cargo test --locked` | Passou o teste existente de handshake duplex com payload de 300 KiB e clone leitor/escritor | `desktop/src-tauri/src/tls.rs` |
| Regressão desktop completa | Windows; `cargo test --locked` | Passou: 45 testes unitários, 1 ignorado, e 1 integração | saída do comando |
| Cliente Android e fila limitada | Windows; `android\.\gradlew.bat testDebugUnitTest assembleDebug` | Passou: 3 testes JVM e APK debug compilado | `android/app/build/outputs/apk/debug/app-debug.apk` |
| Peer que não lê / shutdown sob mutex | Windows; teste `tls::tests::write_timeout_stops_when_peer_stops_reading` | Passou: peer TLS sem leitura recebeu 64 MiB de tentativa, e a escrita terminou com erro em menos de 3 s após timeout de 100 ms | `desktop/src-tauri/src/tls.rs` |
| 100 ciclos de reconexão com payload/ACK/fechamento | Windows; teste `tls::tests::reconnect_cycles_do_not_leave_tls_workers_or_sockets_behind` | Passou: 100 handshakes TLS, payloads, ACKs e encerramentos completos em 0,25 s; o harness não deixou o servidor pendente | `desktop/src-tauri/src/tls.rs` |

- Comparação de desempenho, quando aplicável: ainda não há benchmark; os limites de 5/10/15/180 s são políticas, não medições de latência visual.
- Testes físicos não executados: tablet/USB, Android real, encoder/decoder e sessão longa.
- Critérios de aceite ainda pendentes: medir crescimento de threads/memória com instrumentação externa e executar cancelamento simultâneo/EOF adversarial; os harnesses locais de peer que não lê e 100 ciclos já passaram.

## Operação e reversão

- Como ativar/desativar a mudança: automática em cada sessão; não há flag.
- Como voltar sem perder configurações/pareamentos: reverter somente os timeouts/ownership e o executor Android; nenhum dado persistido foi alterado.
- Limitações conhecidas: o heartbeat depende de PONG válido após PING; vídeo estático não é confundido com ausência de PONG.
- Próximo passo exato: executar em VM um peer com framing parcial/EOF e medir threads/memória; manter T06 em validação até essa evidência externa existir.
