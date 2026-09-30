# Evidências, limites e correções da análise

Base: d823e5b, revisada em 29/09/2026. “Confirmado no código” significa que a implementação foi inspecionada; não significa reprodução em todos os aparelhos.

## Achados rastreáveis

| Achado | Evidência na raiz do repositório | Classificação | Tarefas |
|---|---|---|---|
| Capacidade consultada a 60 Hz, mas Custom permite 90/120 | MainActivity.kt: decodableSize; Stream.kt: newDecoder; server.rs: plan | Confirmado no código | T03 |
| Consulta qualquer decoder, cria decoder pelo MIME | decodableSize usa caps.any; newDecoder usa createDecoderByType | Confirmado no código; falha depende do aparelho | T03, T07 |
| Intervalo reinicia depois de encode + write | server.rs: stream_once, atribuição de last depois de write_msg | Confirmado; perda exata de FPS depende de duração | T04 |
| Dimensão planejada não limita captura no modo Mirror | stream_once calcula w/h; open_capture recebe tablet em vez do tamanho planejado | Confirmado no código | T04 |
| Captura Windows faz readback, cópia e resize CPU | win/capture.rs: next, downscale | Confirmado; custo não medido nesta revisão | T05, T15 |
| Encoder Windows converte BGRA→I420→NV12 em CPU | win/encode.rs: nv12_sample | Confirmado | T15 |
| Encoder Mac copia pixels e completa frames a cada encode | mac/capture.rs: Handler; mac/encode.rs: encode | Confirmado; ganho potencial exige benchmark | T16 |
| Escrita TLS segura mutex enquanto escreve no socket | tls.rs: Write::write, Read::read, shutdown | Confirmado; deadlock/latência exigem teste adversarial | T06 |
| Handshake termina e HELLO fica sem prazo | tls.rs: accept remove timeout; server.rs: handle lê HELLO | Confirmado | T02, T06 |
| Erros de envio Android são descartados | Stream.kt: send, runCatching interno | Confirmado | T06, T07 |
| Decoder espera até 500 ms no leitor da conexão | Stream.kt: decode, freeInputs.poll | Confirmado | T07, T20 |
| Contador “rendered” sobe após tentativa de release | Stream.kt: onOutputBufferAvailable | Confirmado; não prova apresentação física | T05 |
| SessionService não é dono de Stream | MainActivity.kt: campo stream; SessionService.kt | Confirmado; recriação de Activity precisa teste | T09 |
| Surface perdida para decoder, mas PC continua vídeo | Stream.kt: detach; server.rs: stream_once | Confirmado | T10 |
| Serviço usa dataSync, sem onTimeout | AndroidManifest.xml; SessionService.kt; targetSdk 36 | Confirmado; comportamento depende de SO/estado | T09 |
| Certificado é salvo antes de parear e removido em mismatch | MainActivity.kt: onServerCertificate | Confirmado | T11 |
| Bypass de pareamento considera apenas loopback | server.rs: handle | Confirmado; não comprova USB físico | T11, T12 |
| ADB não seleciona serial | lib.rs: usb_forward, install_apk | Confirmado; vários devices podem gerar erro de ambiguidade | T12 |
| Tethering aparece como Wi-Fi | Discovery.kt: Pc.usb; strings.xml: wifi_host; server.rs: session label | Confirmado | T13 |
| Perfil alterado por tablet modifica Settings global | server.rs: PROFILE; settings.rs: VERSION | Comportamento atual confirmado; mudar implica decisão de UX | T18 |
| Estatística principal vem da primeira sessão | lib.rs: status; desktop/src/main.js: refresh | Confirmado | T18 |
| Toda alteração de Settings provoca rebuild | settings.rs: set; server.rs: loop de stream_once | Confirmado, inclusive áudio/toque/onboarding | T18 |
| Default de captura Windows escolhe primeiro output enumerado | win/capture.rs: outputs, find_output | Confirmado; primeiro output não é explicitamente validado como principal | T19 |
| Captura de monitor específico pode cair no principal | server.rs: open_capture | Confirmado; potencial exposição de tela diferente | T19 |
| Áudio usa fila de 25 pacotes de 20 ms | audio.rs: sync_channel e try_send | Limite de ~500 ms; quando cheia descarta novos, preservando antigos | T17 |
| Pipe privilegiado usa lines sem limite de linha | win/service.rs: client, serve, handle | Confirmado; superfície precisa testes de limite/ACL | T34 |
| Windows firewall usa profile=any | windows/hooks.nsh | Confirmado; revisar política, não mudar firewall real nesta tarefa documental | T24, T34 |
| Mac empacota APK, sem adb no config inspecionado | tauri.macos.conf.json; build.yml; lib.rs: adb fallback | Lacuna confirmada no empacotamento; dev pode ter adb no PATH | T12, T24 |
| Compra e contato ainda têm placeholders | license.rs: BUY_URL; docs/privacy.md | Confirmado | T25 |

Os caminhos abreviados Kotlin ficam em android/app/src/main/java/com/tabdisplay/; Rust em desktop/src-tauri/src/.

## Correções importantes à análise inicial

- Usar TCP, sockets ou a permissão INTERNET não exige acesso à internet. Tethering pode criar apenas uma ligação IP local.
- O UDP connect para 8.8.8.8 em lib.rs:status escolhe a rota local; o código não envia um datagrama por essa chamada. Substituí-lo melhora seleção de interfaces/UX; não representa remoção de um stream que ia à internet.
- ADB reverse não identifica sozinho o meio físico; o servidor vê loopback também em outros túneis. Não prometer “cabo é prova” sem verificar a origem.
- Salvar o pin somente depois do código reduz confiança persistida prematuramente, mas não derrota por si só um MITM ativo que retransmite o pareamento. T11 precisa vincular a prova ao canal/identidade.
- Uma fila que descarta vídeo não deve descartar P-frames arbitrariamente. E trocar TCP por UDP não garante mais FPS se o gargalo estiver em captura/encoder/decoder.
- 60 FPS em um método de capabilities não garante 60 FPS sustentados, e 60 no operating-rate é uma indicação, não um limitador universal.
- Bloqueio em socket e tempos de captura/encoder são hipóteses sobre a experiência relatada; não há rastreamento da sessão real do usuário nesta auditoria.

## Verificações já executadas

Na análise anterior desta conversa, nesta base e em Windows:

- cargo test --locked: 20 testes unitários passaram, 1 de loopback de áudio ignorado, 1 teste de integração da assinatura passou.
- gradlew.bat testDebugUnitTest assembleDebug: build passou; testes Android retornaram NO-SOURCE.
- cargo fmt --check: falhou por diferenças de formatação existentes; não houve correção automática.
- git status --short estava limpo antes da criação deste pacote.

Não foram medidos FPS, latência visual, temperatura, consumo, estabilidade por horas nem comportamento no Mac/tablet real. Não apresentar esses resultados como benchmark ou certificação.
