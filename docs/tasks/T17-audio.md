# T17 — Manter áudio recente, estável e sincronizado

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T02, T05, T06, T09.

## Problemas

audio.rs usa 25 pacotes de 20 ms: pode guardar ~500 ms. try_send descarta o pacote novo quando cheia, preservando áudio antigo. O leitor Android também executa decode/play; AudioTrack usa write não bloqueante, cujo retorno não é tratado. Não há timestamps compartilhados de mídia no protocolo atual.

## Implementação

1. Definir orçamento de latência e política de fila em milissegundos, não apenas número de pacotes. Escolher descarte de antigos para baixa latência e recuperação adequada de descontinuidade.
2. Acrescentar sequência/PTS negociados e mapear áudio/vídeo no relógio de mídia da sessão; não confundir PTS sintético com hora real de captura.
3. Separar reprodução/decodificação de áudio da leitura de controle/vídeo com filas pequenas e cancelamento.
4. Tratar retorno parcial/erro de AudioTrack.write; manter somente remanescente ainda dentro do orçamento, ou descartar de modo explícito. Não bloquear leitor tentando preencher todo o buffer.
5. Respeitar offset, tamanho e formato efetivo do output do MediaCodec; tratar INFO_OUTPUT_FORMAT_CHANGED.
6. Definir tratamento de perda/descarte Opus e silêncio, sem assumir que qualquer pacote pode ser removido sem efeito audível.
7. Limpar PCM/packets pendentes ao desligar áudio, mudar saída ou retomar sessão. Não tocar restos antigos depois de mute prolongado.
8. Detectar troca de dispositivo padrão Windows/Mac e reabrir captura com limite; erro deve aparecer em diagnóstico sem derrubar vídeo desnecessariamente.
9. Integrar áudio em background, foco de áudio e rotas fone/Bluetooth conforme APIs Android. Política de áudio não pode alterar volume global do PC.
10. Medir duplicação de captura/encode com dois tablets; compartilhar origem só se houver ganho e ownership/permissão por sessão bem definidos.

## Critérios de aceite

- [ ] Congestionamento não produz meio segundo de áudio velho por desenho da fila.
- [ ] Voltar de mute/pausa não reproduz backlog.
- [ ] Troca de saída/fone e silêncio prolongado têm comportamento previsível.
- [ ] Falha de áudio é observável e não encerra vídeo sem motivo.
- [ ] A/V usa relógio/estratégia de sincronização documentados.

## Testes e limites

Tom, silêncio, palmas/clique audiovisual e escrita parcial simulada; perda, atraso, pause/resume e segundo tablet. Medir diferença audiovisual e latência com o mesmo dispositivo de saída; Bluetooth pode ter atraso próprio, que deve ser identificado.

Rollback preserva opção de áudio desligado e framing legado negociado. Não acelerar reprodução arbitrariamente nem descartar bytes no meio de pacote Opus.
