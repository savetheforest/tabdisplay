# Relatório T21 — Comparação de transportes

Data: 29/09/2026  
Status: Em validação

## Decisão

Manter TCP sobre TLS como transporte padrão nesta versão. Não foi adicionado QUIC, WebRTC, RTP, UDP paralelo ou canal de mídia separado.

O motivo é técnico e verificável no escopo atual: ADB reverse encaminha TCP, o protocolo já tem framing bounded, deadlines, heartbeat, cancelamento, fila de áudio limitada, controle e vídeo na mesma sessão autenticada, e o T20 ainda não possui os benchmarks comparáveis que justificariam a complexidade adicional. “Sem ganho relevante” continua sendo uma conclusão válida, mas ainda não pode ser afirmada sem os números do laboratório.

## Comparação de desenho

| Opção | Vantagem potencial | Custo/risco no TabDisplay | Decisão |
|---|---|---|---|
| TCP/TLS atual | Funciona em Wi‑Fi, tethering e ADB reverse; entrega ordenada e autenticação já integrada | Vídeo pode esperar por bytes anteriores; exige backpressure e deadlines | Manter |
| QUIC stream | Multiplexação e TLS integrado | Não resolve bloqueio de um stream confiável; dependência e suporte Android/Windows/macOS ainda sem matriz | Não adotar sem benchmark |
| QUIC datagram/RTP | Pode descartar mídia atrasada | MTU, fragmentação, reassembly bounded, sequência, IDR e autenticação por canal ainda não existem | Não adotar |
| WebRTC | Ecossistema de mídia e congestionamento | Cliente/dependências grandes, negociação e ICE/STUN/TURN; não atende ao requisito local sem trabalho adicional | Não adotar |
| Canais TCP separados | Controle não espera por vídeo | Requer associação criptográfica por canal; um `session_id` previsível não seria suficiente | Não adotar ainda |

## Contrato preservado

```text
TLS autenticado + token/pareamento
  ├─ framing TCP bounded: HELLO/CONFIG/VIDEO/AUDIO/INPUT/controle
  ├─ leitura tablet → PC: INPUT, RESIZE, PONG, PROFILE, PAUSE/RESUME, KEYFRAME
  └─ escrita PC → tablet: CONFIG/PROFILE, VIDEO, AUDIO, PING/ERROR
```

Controle, input e áudio continuam com políticas distintas dentro da sessão autenticada: o input é lido em thread própria, o vídeo não descarta P-frames arbitrariamente e o áudio tem fila temporal bounded. Qualquer futuro canal separado deverá derivar sua autenticação do handshake TLS e ser encerrado junto com a sessão principal.

## Medições não executadas

O relatório comparativo exigiria a mesma cena, encoder, resolução, bitrate e tablet em LAN estável, Wi‑Fi com perda/jitter, ADB e tethering, medindo throughput, p95, perda, CPU, binário, recuperação e latência A/V. O único tablet foi detectado, mas instalar/abrir o APK atualizado foi bloqueado pela aprovação de mutação do dispositivo; não há laboratório Wi‑Fi/tethering controlado nem segundo tablet disponível nesta execução.

Não foi feito shaping de rede global, não foi aberto acesso no roteador e não foi introduzida criptografia própria. Um protótipo futuro deve ser isolado, ter fallback TCP explícito e não alternar caminhos indefinidamente.
