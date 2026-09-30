# T15 — Pipeline GPU no Windows

## Identificação

- ID e título: T15 — Reduzir cópias e conversões de vídeo na CPU do Windows
- Status: Em validação
- Base: `d823e5b` + T01–T14 parciais + working tree de 29/09/2026
- Plataformas/versões: Windows; crate `windows` 0.61; Rust/cargo 1.98.0
- Dependências verificadas: T03, T04, T05 e T07 implementados localmente; validação física ainda pendente

## Problema e resultado

O caminho anterior sempre copiava a textura duplicada para staging CPU, fazia
readback BGRA, aplicava resize bilinear na CPU quando necessário e entregava
BGRA ao encoder. O encoder Media Foundation continua recebendo BGRA nesta etapa
e fazendo BGRA→I420→NV12 em CPU; não foi alegado zero-copy.

Foi adicionado um bridge `GpuScaler` em `win/capture.rs` que:

- cria um `ID3D11VideoProcessorEnumerator` com dimensões de entrada/saída;
- verifica suporte de BGRA como entrada e saída antes de habilitar o caminho;
- usa `VideoProcessorBlt` para resize no adapter e lê somente o resultado final;
- preserva composição do cursor, inclusive forma colorida, masked/XOR e monocromática,
  desenhando a forma escalada sobre o resultado;
- desabilita o estágio por toda a captura se o driver falhar em runtime e volta ao
  resize bilinear CPU existente.

O limite atual é deliberado: o resultado do VideoProcessor ainda passa por
staging/readback porque a API de encoder da aplicação aceita `&[u8]` BGRA. A
integração de samples DXGI, D3D device manager e conversão NV12 no GPU não foi
inventada sem validar o MFT real.

## Decisões

- A capacidade é testada por adapter e por formato; não se presume que todo
  encoder ou GPU aceite a mesma textura.
- O fallback CPU permanece automático e não altera configurações/pareamentos.
- Falhas de VideoProcessor não entram em retry infinito: o recurso é desligado
  até a próxima captura/rebuild.
- O cursor é composto depois do resize para que uma mudança de cursor não exija
  readback da textura nativa novamente.
- O método não é ativado quando a resolução não muda, evitando custo e objetos
  D3D extras no caso comum.

## Validação

| Verificação/cenário | Ambiente e comando | Resultado observado | Artefato |
|---|---|---|---|
| Compilação e testes unitários desktop | `cargo test --locked` em Windows | Passou: 45 unitários, 1 ignorado, 1 teste de integração; inclui cursor escalado e fallback bilinear | suíte Cargo |
| Capacidade/uso real do VideoProcessor | monitor Windows com captura ativa | Não executado nesta sessão; não foi iniciado stream nem alterada topologia de monitores | pendente |
| GPU, driver e matriz Intel/AMD/NVIDIA | `Get-CimInstance Win32_VideoController` | Tentado novamente; o ambiente retornou `Acesso negado` | pendente |
| 4K→1080p, 2304×1440, virtual, um/dois tablets | release, 30 s + 60 s × 3 | Não medido; tablet atualizado e benchmark físico continuam bloqueados | pendente |
| Cor, texto, cursor, HDR/SDR, rotação e device lost | hardware designado | Não executado | pendente |

Não há números de CPU, FPS, p95, latência, VRAM ou throughput neste relatório.
O teste local confirma apenas compilação, ownership e seleção de fallback; não
certifica suporte de um adapter específico.

## Fontes técnicas

- [ID3D11VideoContext::VideoProcessorBlt](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11videocontext-videoprocessorblt)
- [ID3D11VideoDevice::CreateVideoProcessorInputView](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11videodevice-createvideoprocessorinputview)
- [D3D11_VIDEO_PROCESSOR_STREAM](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_video_processor_stream)

## Operação e reversão

Não há flag pública nova. Para reverter a otimização sem tocar no estado do
usuário, remover o uso de `GpuScaler` em `Capture::open` e manter `full` +
`downscale`; a captura, autenticação, protocolo e fallback do encoder não mudam.

Próximo passo exato: executar o mesmo benchmark de T05 em release com um adapter
real, confirmar suporte do VideoProcessor, comparar resize CPU/GPU e só então
avaliar a etapa separada de NV12/samples DXGI.
