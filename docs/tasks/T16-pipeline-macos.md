# T16 — Otimizar o caminho Mac preservando ownership e compatibilidade

Prioridade: P2. Tipo: Core. Status: Em validação.
Dependências: T03, T04, T05, T07.

## Contexto

ScreenCaptureKit já dimensiona a imagem, mas mac/capture.rs copia pixels para Vec e mac/encode.rs copia de novo para CVPixelBuffer. VTCompressionSessionCompleteFrames é chamado em cada frame. Isso pode limitar paralelismo, mas o impacto precisa medição em Mac.

## Implementação

1. Medir captura/cópias/encode no Apple Silicon de referência e registrar versão do macOS.
2. Avaliar encaminhar CVPixelBuffer/IOSurface retido ao encoder com ownership seguro. Retain/release e thread de callback devem ser auditáveis.
3. Usar fila limitada de frames recentes e pool compatível com VideoToolbox. Não segurar indefinidamente samples do ScreenCaptureKit.
4. Negociar formato de pixel/escala com encoder e preservar fallback BGRA→CPU.
5. Substituir barreira por frame por fluxo assíncrono se o benchmark justificar; manter ordenação, PTS, geração e limite de frames em voo.
6. Usar fps efetivo para minimum frame interval da captura em vez de sempre 120; tratar imagem estática/idle sem perder update final.
7. Propagar status de callback/VTSessionSetProperty/prepare/encode; distinguir encoder hardware efetivo de software fallback.
8. Revisar conversão AVCC→Annex-B, tamanho dos prefixes NAL e parâmetros SPS/PPS em toda transição de configuração/keyframe.
9. Impedir panic em callback C e uso de refcon após liberar sessão; cancelar e aguardar callbacks conforme contrato da API.
10. Testar permissões revogadas, sleep/wake, Retina, display removido e input em pontos versus pixels.
11. Documentar o uso existente de CGVirtualDisplay privada e uma matriz de versões realmente validada; não estender suporte a novos macOS por simples compilação.

## Critérios de aceite

- [ ] Ownership dos buffers/callbacks está documentado e testado sob encerramento.
- [ ] Frames mantêm ordem/PTS e uma AU por mensagem.
- [ ] Usuário vê erro de permissão/encoder com recuperação apropriada.
- [ ] Benchmark mostra efeito das cópias e do modo assíncrono, incluindo consumo.
- [ ] Vídeo, cursor e input Retina permanecem alinhados.

## Testes e rollback

Unitários para Annex-B/AVCC com payload truncado e múltiplos parameter sets; testes em Mac real para buffer/suspensão/duas sessões. Windows não valida essas APIs.

Manter implementação antiga como fallback durante rollout; não deixar duas pipelines capturando simultaneamente sem necessidade. Ausência de Mac impede marcar validação física como concluída.
