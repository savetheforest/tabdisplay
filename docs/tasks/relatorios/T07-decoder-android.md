# T07 — Decoder Android e recuperação de vídeo

## Identificação

- ID e título: T07 — Tornar o decoder Android recuperável e consistente com a Surface
- Status: Em validação
- Base: `d823e5b` + T01–T06 parciais + working tree de 29/09/2026
- Dependências verificadas: T02/T03/T06 implementadas ou em validação; a parte física permanece bloqueada

## Resultado

- Cada item da fila de input carrega a instância concreta de `MediaCodec`; callback antigo não pode fornecer índice para o codec atual.
- A fila de índices é bounded e deduplicada. `configure/start/stop/release` limpam estado e `releaseCodec` tenta liberar mesmo quando `stop` falha.
- A leitura do socket não espera mais por input buffer. A mídia passa por uma fila bounded de quatro access units e um worker separado do thread de callback; a fila cheia registra descarte e pede keyframe com rate limit.
- O tamanho da access unit é comparado com a capacidade do input buffer antes de `put`; buffer ausente, fila cheia ou erro de `queueInputBuffer` aciona recuperação.
- Um pequeno scanner Annex-B mantém o decoder em “esperando IDR” após nova configuração/perda. Access units sem IDR são descartadas até uma access unit IDR válida, sem carregar P-frames órfãos.
- Foi introduzida a mensagem opcional v3 `KEYFRAME` (tipo 16), com `config_id`; o servidor marca rebuild e o próximo CONFIG reabre a geração/encoder. O VIDEO binário não mudou.
- Callbacks de saída verificam a instância ativa e só contam render quando `releaseOutputBuffer(index, true)` tem sucesso. `close`, `detach` e `attach` permanecem idempotentes quanto ao codec.

## Arquivos

- `android/app/src/main/java/com/tabdisplay/Stream.kt`: filas, gerações por identidade de codec, worker de decode, validação de buffer e pedido de keyframe.
- `android/app/src/main/java/com/tabdisplay/H264.kt`: identificação bounded de NAL IDR Annex-B.
- `android/app/src/main/java/com/tabdisplay/Protocol.kt`, `desktop/src-tauri/src/server.rs`, `PROTOCOL.md`: tipo KEYFRAME e reconstrução da sessão.
- `android/app/src/test/java/com/tabdisplay/H264Test.kt`: casos de start code de 3/4 bytes, IDR/P-frame e access units mínimos sem bytes além do NAL.

## Validação

| Verificação | Ambiente/comando | Resultado |
|---|---|---|
| Scanner IDR e framing | Android JVM `testDebugUnitTest` | Passou; casos de Annex-B 3/4 bytes, IDR mínimo de 4/5 bytes, ausência de NAL e não-IDR cobertos |
| Build Android | `android\gradlew.bat testDebugUnitTest assembleDebug` | Passou no Gradle 9.7.1 |
| Servidor aceita KEYFRAME e recompila | `cargo test --locked` | Passou: 41 unitários, 1 ignorado, 1 integração |
| Callback atrasado real, Surface e MediaCodec | Redmi Pad 2 físico | APK release executado; SurfaceView/MediaCodec renderizaram vídeo contínuo por USB, sem `FATAL EXCEPTION`, com CONFIG efetivo 1152x640/60 fps |
| 30 ciclos resize/lock/attach e meta de recuperação | Redmi Pad 2 físico | 30 recriações/reconexões Activity retornaram SurfaceView (Wi‑Fi, após o teste USB); o cenário completo de resize/lock/attach e recuperação em até dois segundos ainda não foi declarado |

## Limitações e reversão

O pedido KEYFRAME reconstrói encoder/configuração porque o contrato atual ainda não possui API de IDR por encoder; isso é fallback explícito e não promete recuperação em até dois segundos. Para reverter, mantenha `config_id`/limites e desative apenas o pedido KEYFRAME/worker por flag interna; não remova a checagem de capacidade do input buffer.
