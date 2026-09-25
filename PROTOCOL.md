# Protocolo TabDisplay

Uma conexão TCP na porta **7070**. O PC é o servidor e o tablet é o cliente.
Cada mensagem tem o formato `[type:u8][len:u32 BE][payload:len bytes]`, com todos os inteiros e floats em big-endian.

| type | nome   | direção     | payload |
|------|--------|-------------|---------|
| 1    | HELLO  | tablet → PC | `width:u32, height:u32, dpi:u32` (primeira mensagem). Tamanho = o maior que o decoder H.264 do tablet aguenta na proporção da tela (Redmi Pad 2: 2304×1440) |
| 2    | VIDEO  | PC → tablet | uma access unit H.264 Annex‑B (SPS/PPS antes de cada IDR) |
| 3    | TOUCH  | tablet → PC | `action:u8 (0 down, 1 move, 2 up), x:f32, y:f32` (0..1 relativo ao vídeo) |
| 4    | CONFIG | PC → tablet | `width:u32, height:u32` do vídeo; (re)cria o decoder. Pode chegar de novo no meio da sessão quando as configurações mudam |

Sequência: tablet envia HELLO → PC envia CONFIG → VIDEO... (o primeiro é keyframe) ⇄ TOUCH.
Mensagens acima de 16 MiB derrubam a conexão.
USB = a mesma conexão via `adb reverse tcp:7070 tcp:7070` (o tablet conecta em 127.0.0.1).

## Descoberta
O PC manda, uma vez por segundo, um broadcast UDP para `255.255.255.255:7071` com o texto `TABDISPLAY <nome do computador>`.
O tablet pega o IP pelo remetente do pacote e lista o PC na tela de conexão.
O USB aparece na lista quando `127.0.0.1:7070` aceita conexão (ou seja, quando o PC já rodou o `adb reverse`).
