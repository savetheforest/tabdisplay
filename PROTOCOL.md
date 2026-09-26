# Protocolo TabDisplay (v2)

Uma conexão TCP na porta **7070**. O PC é o servidor e o tablet é o cliente.
Cada mensagem tem o formato `[type:u8][len:u32 BE][payload:len bytes]`.
Mensagens de controle levam JSON (UTF‑8); VIDEO e INPUT são binárias (big-endian).

| type | nome          | direção     | payload |
|------|---------------|-------------|---------|
| 1    | HELLO         | tablet → PC | `{"v":2, "device_id", "device_name", "token", "screen":[w,h], "decodable":[w,h], "dpi"}` (primeira mensagem) |
| 2    | VIDEO         | PC → tablet | uma access unit H.264 Annex‑B (SPS/PPS antes de cada IDR) |
| 3    | INPUT         | tablet → PC | um quadro de toque/caneta (binário, abaixo) |
| 4    | CONFIG        | PC → tablet | `{"width","height"}` do vídeo; (re)cria o decoder. Chega de novo quando as configurações mudam |
| 5    | PAIR_REQUIRED | PC → tablet | `{"pc_id", "pc_name", "wrong"}`: o PC mostra um código de 6 dígitos; `wrong` = o último não bateu |
| 6    | PAIR          | tablet → PC | `{"code"}` |
| 7    | PAIRED        | PC → tablet | `{"pc_id", "token"}`: o tablet guarda o token por `pc_id` e o manda nos próximos HELLO |
| 8    | ERROR         | PC → tablet | `{"message"}` para mostrar ao usuário; o PC fecha a conexão em seguida |
| 9    | RESIZE        | tablet → PC | `{"decodable":[w,h]}` quando o tablet gira; o PC refaz o monitor no novo formato |
| 10   | PING          | PC → tablet | `{"t"}` (ms desde o início da sessão), uma vez por segundo |
| 11   | PONG          | tablet → PC | o mesmo `{"t"}` de volta: o PC mede a latência de ida e volta |
| 12   | STATS         | tablet → PC | `{"fps"}`: quadros exibidos desde o último PING |
| 13   | SCROLL        | tablet → PC | roda do mouse / rolagem do trackpad, binário (abaixo) |
| 14   | PROFILE       | ambos       | `{"profile":"performance"\|"balanced"\|"quality"\|"auto"\|"custom"}`: o PC informa o perfil de qualidade ativo (logo depois de cada CONFIG); o tablet manda o mesmo para trocá-lo ("custom" só o PC define) e o PC refaz o vídeo, como se fosse trocado na tela do PC |

- `decodable`: o maior tamanho, na proporção da tela, que o decoder H.264 do tablet aguenta (Redmi Pad 2: 2304×1440).
  O PC nunca manda vídeo maior que isso.
- Sequência: HELLO → (PAIR_REQUIRED ⇄ PAIR → PAIRED, só na primeira vez pelo Wi‑Fi) → CONFIG → VIDEO... ⇄ INPUT (+ PING/PONG/STATS a cada segundo, RESIZE ao girar).
- Versão diferente de 2 → ERROR pedindo para atualizar. Mensagens acima de 16 MiB derrubam a conexão.

## INPUT
Um quadro por `MotionEvent` do Android, com todos os contatos daquele instante:
`[count u8]` e, para cada contato, 18 bytes:
`[id u8][kind u8: 0 toque, 1 caneta][action u8: 0 down, 1 move, 2 up, 3 hover, 4 saiu][botões u8: 1 botão da caneta, 2 borracha][x f32][y f32][pressão f32][tilt_x i8][tilt_y i8]`.
x/y vão de 0 a 1 sobre o vídeo; pressão de 0 a 1; inclinação em graus.
O PC reproduz o quadro como toque/caneta nativos do Windows (os gestos vêm do próprio Windows), ou, no modo mouse, só o primeiro dedo move o mouse.

## SCROLL
`[x f32][y f32][dx f32][dy f32]` (big-endian): x/y de 0 a 1 sobre o vídeo (o PC move o cursor até lá antes de rolar) e dx/dy em "cliques" da roda, com o sinal do Android
(`AXIS_HSCROLL`/`AXIS_VSCROLL`): dy > 0 rola para cima, dx > 0 rola para a direita. No Windows vira `MOUSEEVENTF_WHEEL`/`HWHEEL` (120 por clique); no Mac, um evento de scroll de ~40 px por clique.

## Pareamento
Um tablet sem token válido precisa digitar o código que aparece no PC (vale 2 minutos, 5 tentativas) —
pelo Wi‑Fi, e também pelo cabo quando ele chega como rede (compartilhamento de internet pelo USB): a
conexão não chega em 127.0.0.1 nesse caso, então não tem como saber que é o mesmo cabo de uma vez pra
outra sem o token.
Só o USB via `adb reverse` (a conexão chega em 127.0.0.1) não pede pareamento: o cabo com depuração
USB ligada já é prova física.
O tráfego não é criptografado: o token impede que outro aparelho da rede controle o PC, não que alguém escute.

## Descoberta
O PC manda, uma vez por segundo, um broadcast UDP na porta 7071 com o texto
`TABDISPLAY {"id":"<pc_id>","name":"<nome do computador>"}`, para o endereço de broadcast de
*cada* interface de rede que ele tem (Wi‑Fi, Ethernet, e a rede que aparece quando um tablet
compartilha a internet pelo USB) — um broadcast só para `255.255.255.255` sairia apenas pela
interface da rota padrão, que raramente é a do cabo.
O tablet pega o IP pelo remetente do pacote (o IP pode mudar; o `id` não) e lista o PC na tela de
conexão; a rede do cabo aparece assim que o tablet liga **Compartilhar internet pelo USB** (sem
precisar de depuração USB), do mesmo jeito que uma rede Wi‑Fi — inclusive com o mesmo pareamento.
O USB "sem pareamento" aparece na lista quando `127.0.0.1:7070` aceita conexão (o PC mantém o
`adb reverse` ativo; isso exige depuração USB ligada).
