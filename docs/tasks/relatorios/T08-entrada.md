# T08 — Entrada e liberação de contatos

## Identificação

- ID e título: T08 — Garantir entrada correta e liberar contatos ao encerrar
- Status: Em validação
- Base: `d823e5b` + T01–T07 parciais + working tree de 29/09/2026

## Resultado

- `ACTION_CANCEL` agora é o estado de protocolo `Cancel` (5), em vez de ser convertido silenciosamente em `Up`; o injetor o trata como liberação sem novo clique.
- O cliente Android limita o quadro a dez contatos, alinhado ao dispositivo touch sintético Windows; parser Rust e codec Kotlin rejeitam valores acima desse limite operacional.
- O modo mouse guarda o `pointerId` que iniciou o gesto e ignora dedos posteriores até o término do mesmo gesto. Não há troca implícita para o primeiro contato restante.
- `Injector` mantém somente contatos/botões criados pela própria sessão e, no `Drop`, envia liberações para touch, pen e mouse antes de destruir os dispositivos sintéticos.
- Erros de `InjectSyntheticPointerInput` e `SendInput` agora entram em warning; a ausência de controle não é tratada como vídeo desconectado.
- Validações existentes continuam cobrindo IDs únicos, enum, finitude, coordenadas, pressão, inclinação e tamanho de payload; a fila de saída do Android permanece bounded.

## Arquivos

- `android/app/src/main/java/com/tabdisplay/Input.kt`: CANCEL e limite operacional.
- `desktop/src-tauri/src/input.rs`: `Action::Cancel` e `MAX_CONTACTS`.
- `desktop/src-tauri/src/win/input.rs`: ownership do gesto mouse, rastreamento/liberação por sessão e erros de injeção.
- `PROTOCOL.md`: contrato de ação e limite do cliente Windows.

## Validação

| Verificação | Ambiente/comando | Resultado |
|---|---|---|
| Parser Rust, fixtures e limites | `cargo test --locked` | Passou: 26 unitários, 1 ignorado, 1 integração |
| Codec/adaptador Android | `android\gradlew.bat testDebugUnitTest assembleDebug` | Passou no Gradle 9.7.1 |
| Liberação em Drop e dois dedos no modo mouse | sessão release com Redmi Pad 2 USB | Toque/arraste central no modo nativo manteve vídeo e sessão; o modo mouse reproduziu `SendInput=0`/erro 87 nesta máquina, sem desconectar, e requer processo instalado/elevado ou investigação UIPI |
| Letterbox, múltiplos monitores e cantos | sessão visual física | SurfaceView e toque/arraste central passaram; letterbox/cantos/múltiplos monitores continuam pendentes |
| Permissão/controle macOS | Mac com permissões reais | Pendente; limitações de mouse/gestos continuam documentadas |

## Limitações e reversão

O limite de dez contatos é a capacidade escolhida para o dispositivo sintético Windows e não afirma suporte universal a outros injetores. O caminho macOS e hover de mouse externo permanecem fora de uma alegação de suporte completo. Reversão segura é desligar o modo de controle, sempre liberando o estado da sessão; não remover `Cancel`, a validação de tamanho ou a liberação no `Drop`.

Evidência física adicional em 29/09/2026: APK release instalado sem limpar dados; pareamento inicial e reconexão pelo `adb reverse` passaram; sessão reportou 1152x640, 60 fps e 10 Mbps no PC. A preferência persistida `touch_mode=mouse` foi restaurada após o teste nativo.
