# T03 — Negociação de decoder, resolução e FPS

## Identificação

- ID e título: T03 — Negociar um modo de vídeo que o decoder selecionado suporte
- Status: Em validação
- Commit base e revisão testada: `d823e5b` + T01/T02/T06 parcial + working tree de 29/09/2026
- Plataformas/versões: Windows; Android API 36 no dispositivo detectado; Rust/cargo 1.98.0; Gradle 9.7.1
- Dependências verificadas: T02 concluída; fixtures e framing de T01 disponíveis

## Problema e resultado

- Cenário que disparava o problema: `MainActivity` consultava qualquer decoder AVC a 60 Hz, mas `Stream` criava outro pelo MIME; Custom podia enviar 90/120 sem o decoder escolhido ter aprovado essa combinação.
- Evidência antes: `caps.any` em `decodableSize`, `createDecoderByType` em `newDecoder` e `KEY_OPERATING_RATE=60` fixo.
- Comportamento depois: o Android enumera decoders AVC não seguros/não tunneled, registra nome concreto/canônico, hardware/software e uma lista finita de modos (30/60/90/120 quando `areSizeAndRateSupported` aprova); HELLO leva `video_modes`; o PC cruza o modo solicitado com os limites anunciados e envia CONFIG com FPS efetivo; o decoder é criado pelo nome e tenta candidatos limitados, liberando candidatos que falham.
- Arquivos e símbolos alterados: `Video.kt`, `MainActivity.kt`, `Stream.kt`, `server.rs`, `PROTOCOL.md`, fixture HELLO e testes JVM/Rust.

## Decisões

- Contratos de API/protocolo e compatibilidade: extensão opcional v3 `video_modes`; clientes que não enviam a lista usam um modo conservador a 60 Hz. CONFIG ganhou `fps` efetivo; cliente aceita ausência como 60.
- Ownership, limites, timeouts e cancelamento: lista de modos é limitada a 32 no servidor; cada configuração usa dimensões validadas e FPS 1..240; falhas de configuração liberam o candidato antes da próxima tentativa.
- Dados persistidos e migração: nenhum.
- Alternativa descartada e motivo, se relevante: não foi usado `createDecoderByType`, pois a documentação oficial recomenda selecionar pelo nome para garantir que as capabilities e o componente instanciado sejam o mesmo; performance points ausentes não são tratados como falha ou garantia.

## Validação

| Verificação/cenário | Ambiente e comando | Resultado observado | Artefato |
|---|---|---|---|
| Capacidades fictícias: hardware/software, 60 vs 90 e fallback | Windows JVM; `android\.\gradlew.bat testDebugUnitTest` | Passou: `VideoNegotiationTest` seleciona decoder concreto e nunca inventa 90 Hz | `android/app/src/test/java/com/tabdisplay/VideoNegotiationTest.kt` |
| PC cruza capacidade efetiva | Windows; `cargo test --locked` | Passou: 24 testes unitários, 1 ignorado, e 1 integração; modo Custom a 120 cai para 60 quando só 60 foi anunciado | `desktop/src-tauri/src/server.rs` |
| Android build | Windows; `android\.\gradlew.bat testDebugUnitTest assembleDebug` | Passou; APK debug em `android/app/build/outputs/apk/debug/app-debug.apk` | APK debug |
| Release assinado | Windows; `android\.\gradlew.bat assembleRelease` + `apksigner verify --print-certs` | Passou; certificado coincide com o app instalado: `9ce42a95…79086b0` | `android/app/build/outputs/apk/release/app-release.apk` |
| Tablet real detectado | ADB bundled, serial `kfxkfin7nbpfq4q8` | Redmi Pad 2 `25040RP0AE`, API 36, MediaTek `mt6789`, estado `device`; sem instalar ainda | diagnóstico ADB |
| Instalação/decoder físico | ADB `install -r` + abertura | Não executado: aprovação de mutação no tablet foi rejeitada; não houve desinstalação/limpeza | bloqueio externo |

- Comparação de desempenho, quando aplicável: não executada; não há FPS físico/decoder real observado.
- Testes físicos não executados: instalação/abertura da nova versão e seleção real de decoder/Surface; a etapa exige autorização explícita da aprovação de dispositivo.
- Critérios de aceite ainda pendentes: confirmar no tablet que o decoder selecionado e a Surface aceitam os modos anunciados e testar rotação sem ciclo; o restante da negociação possui testes puros.

## Operação e reversão

- Como ativar/desativar a mudança: automática na criação da sessão; sem flag.
- Como voltar sem perder configurações/pareamentos: usar cliente/servidor v3 sem `video_modes` (fallback 60) ou reverter somente os adaptadores; nenhum dado persistido foi alterado.
- Limitações conhecidas: `video_modes` é calculado para a tela observada no HELLO; o tablet físico ainda não confirmou suporte real do caminho MediaCodec/Surface.
- Próximo passo exato: depois de obter autorização explícita para instalar no tablet, executar a sessão física com o serial acima; em paralelo, continuar T04 sem declarar T03 totalmente concluída.
