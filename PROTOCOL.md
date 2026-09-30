# Protocolo TabDisplay (v3)

Uma conexão TCP na porta **7070**, dentro de **TLS 1.3** (ver "Segurança"). O PC é o servidor e o tablet é o cliente.
Cada mensagem tem o formato `[type:u8][len:u32 BE][payload:len bytes]`.
Mensagens de controle levam JSON (UTF‑8); VIDEO e INPUT são binárias (big-endian).

| type | nome          | direção     | payload |
|------|---------------|-------------|---------|
| 1    | HELLO         | tablet → PC | `{"v":3, "device_id", "device_name", "token", "screen":[w,h], "decodable":[w,h], "video_modes":[{"width","height","fps"}], "audio_timestamps":true, "text_transfer":true, "keyboard":true, "dpi"}` (primeira mensagem; capabilities ausentes = desligadas) |
| 2    | VIDEO         | PC → tablet | uma access unit H.264 Annex‑B (SPS/PPS antes de cada IDR) |
| 3    | INPUT         | tablet → PC | um quadro de toque/caneta (binário, abaixo) |
| 4    | CONFIG        | PC → tablet | `{"width","height","fps","config_id"}` efetivos do vídeo; (re)cria o decoder. `config_id` é monotônico na sessão e associa callbacks/métricas à geração; clientes antigos podem omiti-lo |
| 5    | PAIR_REQUIRED | PC → tablet | `{"pc_id", "pc_name", "wrong"}`: o PC mostra um código de 6 dígitos; `wrong` = o último não bateu |
| 6    | PAIR          | tablet → PC | `{"code"}` |
| 7    | PAIRED        | PC → tablet | `{"pc_id", "token"}`: o tablet guarda o token por `pc_id` e o manda nos próximos HELLO |
| 8    | ERROR         | PC → tablet | `{"message"}` para mostrar ao usuário; o PC fecha a conexão em seguida |
| 9    | RESIZE        | tablet → PC | `{"decodable":[w,h]}` quando o tablet gira; o PC refaz o monitor no novo formato |
| 10   | PING          | PC → tablet | `{"t"}` (ms desde o início da sessão), uma vez por segundo |
| 11   | PONG          | tablet → PC | o mesmo `{"t"}` de volta: o PC mede a latência de ida e volta |
| 12   | STATS         | tablet → PC | `{"fps","decode_fps","render_fps","decode_drops","render_failures","config_id"}`: contadores desde o último PING; render é evidência do callback de saída, não prova de luz emitida no painel |
| 13   | SCROLL        | tablet → PC | roda do mouse / rolagem do trackpad, binário (abaixo) |
| 14   | PROFILE       | ambos       | `{"profile":"performance"\|"balanced"\|"quality"\|"auto"\|"custom"}`: o PC informa o perfil efetivo desta sessão (logo depois de cada CONFIG); o tablet autenticado manda o mesmo para trocar somente a própria sessão ("custom" só o PC define) e o PC refaz o vídeo dessa sessão |
| 15   | AUDIO         | PC → tablet | pacote Opus; com `audio_timestamps:true`: `[version:u8=1][sequence:u64 BE][pts_samples:u64 BE][opus]`; sem o campo: somente Opus |
| 16   | KEYFRAME      | tablet → PC | `{"config_id"}`: solicita reconstrução limitada do encoder após perda de estado; o próximo CONFIG inicia uma nova geração |
| 17   | PAUSE         | tablet → PC | `{}`: Surface não está visível; interrompe captura/encode mantendo a sessão e o monitor virtual |
| 18   | RESUME        | tablet → PC | `{}`: Surface voltou; libera a pausa e pede CONFIG/IDR novo sem reproduzir backlog |
| 19   | TEXT          | ambos       | `{"version":1, "source":"tablet"|"pc", "text":"..."}`: transferência manual de texto autenticada, somente quando a capability `text_transfer` foi anunciada |
| 20   | KEY           | tablet → PC | `{"version":1, "source":"tablet", "action":"text", "text":"..."}` ou `action:"key", "key":"Enter"|"Backspace"|"Tab"|"Escape"|"Delete"|"ArrowLeft"|"ArrowRight"|"ArrowUp"|"ArrowDown"|"Home"|"End"|"Space", "modifiers":0..15, "down":true|false}`; somente com `keyboard` e “Controlar o PC” ligados |

- `decodable`: o maior tamanho, na proporção da tela, que o decoder H.264 do tablet aguenta (Redmi Pad 2: 2304×1440).
  O PC nunca manda vídeo maior que isso.
- Sequência: HELLO → (PAIR_REQUIRED ⇄ PAIR → PAIRED, só na primeira vez pelo Wi‑Fi) → CONFIG → VIDEO... ⇄ INPUT (+ PING/PONG/STATS a cada segundo, RESIZE ao girar, KEYFRAME quando o decoder perde estado).
- Versão diferente de 3 → ERROR pedindo para atualizar. Mensagens acima de 16 MiB derrubam a conexão.

## Estados, limites e erros

O envelope continua `[type:u8][len:u32 BE][payload]`. A conexão passa por estes estados; uma mensagem fora da coluna permitida é rejeitada sem iniciar captura ou driver:

| Estado | Mensagens aceitas | Próximo estado / erro |
|---|---|---|
| TLS | nenhuma mensagem de aplicação até o TLS terminar | falha de TLS encerra |
| Aguardando HELLO | HELLO exatamente uma vez | versão incompatível ou HELLO inválido → ERROR e fechamento |
| Pareamento | PAIR; o PC envia PAIR_REQUIRED/PAIRED | código inválido mantém o estado; limite/timeout encerra |
| Configurando | RESIZE, INPUT e PROFILE do tablet | CONFIG/PROFILE do PC iniciam a sessão; dados inválidos são ignorados ou encerram conforme o tipo |
| Streaming | VIDEO/AUDIO/PING/TEXT do PC; INPUT/SCROLL/RESIZE/PROFILE/PONG/STATS/KEYFRAME/PAUSE/RESUME/TEXT/KEY do tablet | RESIZE/PROFILE/KEYFRAME/RESUME refazem a configuração; PAUSE interrompe mídia; TEXT/KEY só são aceitos com capability e ação explícita; EOF entra em encerramento |
| Pausado | nenhuma captura no tablet; a sessão pode permanecer conectada | retomada envia CONFIG e keyframe coerentes |
| Encerrando | nenhuma | fechamento idempotente e liberação de recursos |

Limites são verificados pelo tipo antes de alocar o payload, além do teto global de 16 MiB:

| Campo | Limite/validade |
|---|---|
| HELLO/controle/erro | 64 KiB por mensagem JSON |
| VIDEO | 8 MiB por access unit |
| AUDIO | 256 KiB por pacote |
| TEXT | texto UTF-8 até 16 KiB; JSON total continua limitado a 64 KiB |
| KEY | JSON até 64 KiB; texto commitado até 16 KiB; somente teclas lógicas e quatro modificadores conhecidos |
| INPUT | no máximo 255 contatos (4591 bytes), IDs únicos, enum conhecido e coordenadas/pressão finitas em 0..1 |
| SCROLL | exatamente 16 bytes; posição finita em 0..1 |
| `device_id`/`device_name`/token | respectivamente não vazios e até 128/128/512 bytes |
| dimensões | 16..7680, múltiplas de 16, produto até 16.777.216 pixels |
| DPI/FPS/bitrate | DPI 72..1000; FPS 1..240; bitrate 1..200 Mbps |

Um tipo de mensagem desconhecido, tamanho acima do limite do tipo, framing truncado ou JSON obrigatório inválido encerra a conexão com erro controlado. Campos JSON adicionais são ignorados somente quando a mensagem-base continua válida; eles não criam capacidade ou autorização implícita.

`TEXT` é um MVP de clipboard manual: o tablet lê o item primário somente ao tocar em “Enviar texto copiado”, e o PC lê sua área de transferência somente ao clicar no botão correspondente. O receptor mostra uma confirmação e só altera seu clipboard ao tocar em “Copiar”; não há paste, sync contínuo, histórico, execução, abertura de URL, persistência ou telemetria do conteúdo. Texto inválido, direção incorreta ou acima de 16 KiB é descartado; o inbox do PC é apenas memória transitória e contém no máximo o último item recebido.

`KEY` separa texto commitado pelo IME de teclas lógicas. O tablet não envia Android keycodes, scancodes ou macros: a lista de teclas e modificadores é allow-listada no PC, a capability só é anunciada por cliente atualizado e o servidor ignora o canal quando “Controlar o PC” está desligado. Texto é enviado apenas ao tocar “Enviar texto”; não há execução arbitrária, shell ou automação recebida do peer.

### Compatibilidade e extensões

O protocolo implementado é v3. Um `v` diferente de 3 no HELLO resulta em ERROR útil e fechamento nos dois sentidos; não há downgrade para plaintext. Extensões opcionais de v3 precisam de campo/versionamento documentado, valor padrão seguro e fixture nos dois clientes antes de serem enviadas. Tipos novos não são ignorados silenciosamente.

`config_id` é um campo opcional de CONFIG/STATS v3; não altera o VIDEO binário e o valor zero representa cliente legado. Pedido explícito de IDR continua sendo uma extensão já documentada. O VIDEO continua sendo uma access unit H.264 Annex-B, sem cabeçalho adicional. O AUDIO versionado usa PTS em amostras de 48 kHz, relativo ao início da sessão; não é hora de parede nem uma medição de latência visual.

As métricas locais do PC separam captura, encode e escrita TLS em uma janela limitada; p50/p95/p99 são calculados somente sobre amostras recentes. O cliente separa submissão ao MediaCodec de render observado e informa descartes/falhas. Nenhum relógio do PC é subtraído do relógio do tablet.

## INPUT
Um quadro por `MotionEvent` do Android, com todos os contatos daquele instante:
`[count u8]` e, para cada contato, 18 bytes:
`[id u8][kind u8: 0 toque, 1 caneta][action u8: 0 down, 1 move, 2 up, 3 hover, 4 saiu, 5 cancel][botões u8: 1 botão da caneta, 2 borracha][x f32][y f32][pressão f32][tilt_x i8][tilt_y i8]`.
x/y vão de 0 a 1 sobre o vídeo; pressão de 0 a 1; inclinação em graus.
O envelope aceita até 255 contatos, mas o cliente Windows limita o quadro a 10 para coincidir com a capacidade do dispositivo sintético.
O PC reproduz o quadro como toque/caneta nativos do Windows (os gestos vêm do próprio Windows), ou, no modo mouse, só o primeiro dedo move o mouse.

## SCROLL
`[x f32][y f32][dx f32][dy f32]` (big-endian): x/y de 0 a 1 sobre o vídeo (o PC move o cursor até lá antes de rolar) e dx/dy em "cliques" da roda, com o sinal do Android
(`AXIS_HSCROLL`/`AXIS_VSCROLL`): dy > 0 rola para cima, dx > 0 rola para a direita. No Windows vira `MOUSEEVENTF_WHEEL`/`HWHEEL` (120 por clique); no Mac, um evento de scroll de ~40 px por clique.

## AUDIO
O PC captura o que toca no alto-falante (WASAPI loopback no Windows, ScreenCaptureKit no Mac), codifica em Opus (48 kHz, estéreo, ~128 kbps, 20 ms) e manda um pacote por mensagem. Clientes novos recebem sequência/PTS; sem som tocando não há pacotes. A fila do PC é limitada a 160 ms e descarta os pacotes mais antigos quando congestionada. O envio pode ser desligado nas configurações do PC; ao desligar, pausar ou retomar, o backlog pendente é limpo.

## Pareamento
Um tablet sem token válido precisa digitar o código que aparece no PC (vale 2 minutos, 5 tentativas) —
pelo Wi‑Fi, e também pelo cabo quando ele chega como rede (compartilhamento de internet pelo USB): a
conexão não chega em 127.0.0.1 nesse caso, então não tem como saber que é o mesmo cabo de uma vez pra
outra sem o token.
Mesmo o USB via `adb reverse` (a conexão chega em 127.0.0.1) segue o pareamento: loopback também pode
ser criado por depuração sem fio ou outro processo local e não prova um cabo específico. Depois do
pareamento o token vai em cada HELLO (já dentro do TLS).

## Segurança
Toda a sessão (HELLO, token, vídeo, áudio, input) vai dentro de TLS 1.3. O PC gera um certificado autoassinado na primeira execução (`cert.der`/`key.der` na pasta de configuração) e o mantém. O tablet aceita o certificado no primeiro contato e **fixa** o SHA-256 dele por PC (confiança no primeiro uso, como o token de pareamento). Se o certificado mudar depois (PC reinstalado ou outro computador respondendo), o tablet recusa, esquece o PC e o token, e a próxima conexão pede o código de pareamento de novo.
O primeiro contato é o ponto fraco do modelo: quem estiver no meio da rede exatamente nessa hora pode se passar pelo PC. ADB loopback e rede USB também não substituem o pareamento; loopback não prova sozinho qual cabo ou processo criou a conexão.

Transição: um tablet da versão 2 (sem TLS) que conecta num PC novo recebe um ERROR em texto puro pedindo para atualizar o app; um tablet novo num PC antigo não consegue o handshake e mostra "atualize o TabDisplay no PC e no tablet".

## Descoberta
O PC manda, uma vez por segundo, um broadcast UDP na porta 7071 com o texto
`TABDISPLAY {"id":"<pc_id>","name":"<nome do computador>"}`, para o endereço de broadcast de
*cada* interface de rede que ele tem (Wi‑Fi, Ethernet, e a rede que aparece quando um tablet
compartilha a internet pelo USB) — um broadcast só para `255.255.255.255` sairia apenas pela
interface da rota padrão, que raramente é a do cabo.
O tablet pega o IP pelo remetente do pacote (o IP pode mudar; o `id` não) e lista o PC na tela de
conexão; a rede do cabo aparece assim que o tablet liga **Compartilhar internet pelo USB** (sem
precisar de depuração USB), do mesmo jeito que uma rede Wi‑Fi — inclusive com o mesmo pareamento.
A conexão local pelo ADB aparece na lista quando `127.0.0.1:7070` aceita conexão (o PC mantém o
`adb reverse` ativo; isso exige depuração USB ligada). Ela ainda segue o pareamento e pinning normais.
