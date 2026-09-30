# T10 — Pausa, retomada e energia

## Identificação

- ID e título: T10 — Pausar vídeo e ajustar consumo sem perder a sessão
- Status: Em validação
- Base: `d823e5b` + T01–T09 parciais + working tree de 29/09/2026

## Resultado

- A perda da Surface envia `PAUSE`; o PC mantém TLS, heartbeat e monitor virtual, mas encerra a captura/encoder da geração corrente. Não há produção sustentada de vídeo durante a pausa.
- O retorno envia `RESUME`; o PC cria uma configuração/encoder novo e o primeiro fluxo começa em nova geração/IDR. Nenhum backlog de access units é reproduzido.
- `attach` sem mudança de tamanho não envia `RESIZE`; apenas retoma. Rotação continua usando RESIZE somente quando a dimensão decodível mudou.
- `SessionService` mantém notificação e sessão durante pausa, mas libera o wake lock; retoma readquire o lock. `FLAG_KEEP_SCREEN_ON` fica ativo somente enquanto há Surface visível.
- O áudio continua independente conforme a política atual: silenciar no tablet não altera volume do PC. A pausa de vídeo não foi declarada como pausa de áudio.
- O serviço usa `connectedDevice`, não `dataSync`, para a conexão contínua com o tablet. `dataSync` teria limite de seis horas em 24 horas no Android 15 quando em background; essa escolha e o comportamento de timeout estão documentados em T09.

## Arquivos e protocolo

- `desktop/src-tauri/src/server.rs`: estado paused, suspensão do loop de mídia e reconstrução no RESUME.
- `android/app/src/main/java/com/tabdisplay/Stream.kt`: PAUSE/RESUME, limpeza de fila e retomada sem RESIZE redundante.
- `android/app/src/main/java/com/tabdisplay/MainActivity.kt`, `SessionService.kt`: tela ligada e wake lock somente durante uso.
- `PROTOCOL.md`: mensagens PAUSE/RESUME e estados.

## Validação

| Verificação | Ambiente/comando | Resultado |
|---|---|---|
| Servidor e estado de pausa compiláveis | `cargo test --locked` | Passou: 45 unitários, 1 ignorado, 1 integração |
| Cliente/manifesto/FGS | `android\gradlew.bat testDebugUnitTest assembleDebug` | Passou no Gradle 9.7.1 |
| Pausa/resume real, lock/unlock e duas sessões | Redmi Pad 2 release, USB | Um ciclo Home→retomada manteve a conexão e o Surface voltou sem tela preta; duas sessões, lock/unlock repetido e cabo removido pausado continuam pendentes |
| 10 min ativo versus 10 min pausado e térmico | Redmi Pad 2 release, USB | Benchmark estático executou 30 s de aquecimento + três coletas; ao final bateria 38 °C, pele 41,3 °C, status térmico normal. A comparação de 10 min ativo/pausado e economia energética ainda não foi declarada |

## Limitações e reversão

Não há leitura de temperatura nem perfil de economia persistente nesta etapa; não se afirma redução térmica. O controlador de sessão ainda é acionado pela Activity, embora o serviço preserve a conexão. Reversão segura é desativar PAUSE/RESUME atrás de flag e manter heartbeat/cleanup; não manter wake lock infinito para esconder incompatibilidade.
