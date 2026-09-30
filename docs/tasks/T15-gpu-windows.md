# T15 — Reduzir cópias e conversões de vídeo na CPU do Windows

Prioridade: P2. Tipo: Core. Status: Em validação.
Dependências: T03, T04, T05, T07.

## Contexto

win/capture.rs lê a textura para staging CPU, copia para clean/full e faz downscale bilinear. win/encode.rs converte BGRA→I420→NV12 em CPU e cria buffers por frame. O encoder GPU não torna esse percurso inteiro acelerado.

## Etapas de implementação

1. Medir baseline release por etapa em Mirror 4K→1080p, 2304×1440 e modo virtual, com um e dois tablets.
2. Especificar frame de plataforma com ownership explícito para textura GPU ou buffer CPU. Não expor tipos Windows aos módulos Mac/Android.
3. Implementar resize e BGRA→NV12 em D3D11 VideoProcessor ou caminho suportado equivalente; validar formatos/capacidades antes de usar.
4. Integrar Media Foundation com D3D device manager e samples DXGI quando o MFT suportar. Não presumir que todo encoder de hardware aceita textura.
5. Escolher encoder/adapter coerente com captura. Em máquina híbrida, documentar fallback/cópia entre adapters e testar monitor em cada GPU.
6. Preservar cursor: formas coloridas, monocromáticas e masked/XOR. Compor na GPU ou manter fallback específico, evitando cursor ausente/duplicado.
7. Reusar pools de texturas/buffers sem reutilizar memória ainda referenciada por encoder assíncrono. Limitar frames em voo.
8. Tratar DXGI access/device lost, stream change, need input/have output, erro de evento e EOS. Substituir espera potencialmente infinita por cancelamento.
9. Obedecer tamanho/alinhamento de output informado pelo MFT; não assumir buffer w*h nem suporte de um único encoder enumerado.
10. Garantir uma access unit por mensagem, com timestamps e SPS/PPS/IDR corretos. Não juntar saídas de frames distintos em um único VIDEO por conveniência.
11. Garantir pares COM/MF startup/shutdown e liberação em falha parcial; evitar ponteiro nulo mesmo em listas vazias retornadas por FFI.
12. Manter fallback CPU funcional e motivo visível. Em Auto, recuperação de erro de runtime pode reabrir pipeline com limite; GPU obrigatório reporta falha honestamente.

## Critérios de aceite

- [ ] Caminho acelerado evita readback/conversão CPU que o benchmark identificou.
- [ ] Cores, faixa YUV, proporção, texto fino e cursor permanecem corretos.
- [ ] Não há crescimento sustentado de memória/VRAM nem buffer reutilizado cedo.
- [ ] NVIDIA/AMD/Intel têm resultados identificados; hardware ausente fica pendente.
- [ ] Comparação inclui CPU, encode/resize p95, FPS e latência, não apenas taxa média.
- [ ] Fallback funciona com hardware encoder indisponível.

## Validação e rollout

Usar padrões visuais/cursor, monitores rotacionados, HDR ligado com saída SDR explicitamente definida, GPUs híbridas, troca de resolução e UAC/lock. Não transmitir secure desktop nem usar privilégios adicionais para contornar bloqueios.

Ativar inicialmente por opção interna/flag, mantendo fallback. Promover a padrão somente após critérios e teste prolongado de VALIDACAO.md. Se o ganho não compensar regressões, registrar e reverter a otimização.

Rollback: desativar o caminho GPU novo, aguardar/cancelar frames em voo e liberar texturas antes de reabrir o fallback. Preservar negociação, limite de filas e correções de pacing já integradas. Não restaurar arquivos inteiros de uma revisão antiga por cima de mudanças de outras tarefas.
