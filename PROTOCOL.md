# Protocolo TabDisplay (v2)

Uma conexão TCP na porta **7070**. O PC é o servidor e o tablet é o cliente.
Cada mensagem tem o formato `[type:u8][len:u32 BE][payload:len bytes]`.
Mensagens de controle levam JSON (UTF‑8); VIDEO e TOUCH são binárias (big-endian).

| type | nome          | direção     | payload |
|------|---------------|-------------|---------|
| 1    | HELLO         | tablet → PC | `{"v":2, "device_id", "device_name", "token", "screen":[w,h], "decodable":[w,h], "dpi"}` (primeira mensagem) |
| 2    | VIDEO         | PC → tablet | uma access unit H.264 Annex‑B (SPS/PPS antes de cada IDR) |
| 3    | TOUCH         | tablet → PC | `action:u8 (0 down, 1 move, 2 up), x:f32, y:f32` (0..1 relativo ao vídeo) |
| 4    | CONFIG        | PC → tablet | `{"width","height"}` do vídeo; (re)cria o decoder. Chega de novo quando as configurações mudam |
| 5    | PAIR_REQUIRED | PC → tablet | `{"pc_id", "pc_name", "wrong"}`: o PC mostra um código de 6 dígitos; `wrong` = o último não bateu |
| 6    | PAIR          | tablet → PC | `{"code"}` |
| 7    | PAIRED        | PC → tablet | `{"pc_id", "token"}`: o tablet guarda o token por `pc_id` e o manda nos próximos HELLO |
| 8    | ERROR         | PC → tablet | `{"message"}` para mostrar ao usuário; o PC fecha a conexão em seguida |

- `decodable`: o maior tamanho, na proporção da tela, que o decoder H.264 do tablet aguenta (Redmi Pad 2: 2304×1440).
  O PC nunca manda vídeo maior que isso.
- Sequência: HELLO → (PAIR_REQUIRED ⇄ PAIR → PAIRED, só na primeira vez pelo Wi‑Fi) → CONFIG → VIDEO... ⇄ TOUCH.
- Versão diferente de 2 → ERROR pedindo para atualizar. Mensagens acima de 16 MiB derrubam a conexão.

## Pareamento
Pelo Wi‑Fi, um tablet sem token válido precisa digitar o código que aparece no PC (vale 2 minutos, 5 tentativas).
Pelo USB (`adb reverse`, a conexão chega em 127.0.0.1) não há pareamento: o cabo já é prova física.
O tráfego não é criptografado: o token impede que outro aparelho da rede controle o PC, não que alguém escute.

## Descoberta
O PC manda, uma vez por segundo, um broadcast UDP para `255.255.255.255:7071` com o texto
`TABDISPLAY {"id":"<pc_id>","name":"<nome do computador>"}`.
O tablet pega o IP pelo remetente do pacote (o IP pode mudar; o `id` não) e lista o PC na tela de conexão.
O USB aparece na lista quando `127.0.0.1:7070` aceita conexão (o PC mantém o `adb reverse` ativo).
