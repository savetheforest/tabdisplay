# Fontes técnicas

Consultadas em 29/09/2026. Revalidar quando a tarefa for implementada; os contratos no código e as versões fixadas continuam sendo a referência para compatibilidade.

## Android

- [MediaCodecInfo.VideoCapabilities](https://developer.android.com/reference/android/media/MediaCodecInfo.VideoCapabilities): limites por tamanho/taxa e estimativas de desempenho. Informação ausente não deve provocar crash nem virar garantia de FPS. Usada em T03/T05.
- [MediaCodec](https://developer.android.com/reference/android/media/MediaCodec): ciclo de buffers, callbacks e observação de renderização. Usada em T07.
- [Informações de codecs](https://developer.android.com/media/optimize/performance/codec?hl=en): diferenciação hardware/software e performance points. Usada em T03/T29.
- [Tipos de foreground service](https://developer.android.com/develop/background-work/services/fgs/service-types): cada tipo exige finalidade e pré-requisitos próprios. connectedDevice é candidato a avaliar para interação com PC; não basta trocar o Manifest sem conferir o fluxo. Usada em T09.
- [Timeouts de foreground service](https://developer.android.com/develop/background-work/services/fgs/timeout): dataSync tem limites de tempo em segundo plano nas condições descritas pela plataforma. Tratar encerramento e testar em versão aplicável. Usada em T09.
- [Mudanças do Android 15 para apps com target 35+](https://developer.android.com/about/versions/15/behavior-changes-15): prevê limite de seis horas em 24 para dataSync, reset ao voltar ao primeiro plano e callback de timeout. Isso não limita toda sessão visível a seis horas. Usada em T09/T23.
- [USB accessory](https://developer.android.com/develop/connectivity/usb/accessory): suporte, autorização e I/O no Android. Nem todo dispositivo implementa accessory mode. Usada em T14.
- [AOA 1.0](https://source.android.com/docs/core/interaction/accessories/aoa): negociação do modo accessory e endpoints. Usada em T14; não representa promessa de velocidade ou compatibilidade com todo tablet.
- [Manual oficial do ADB](https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/docs/user/adb.1.md): seleção de dispositivo, reverses e transportes. Usada em T12.

## Fontes a consultar no momento da implementação

Para as tarefas abaixo, registrar na entrega a página oficial efetivamente consultada e a versão relevante. Não afirmar que a API está disponível sem verificar.

- T15: Microsoft Learn, Desktop Duplication, Media Foundation/D3D11 device manager, sample buffers DXGI, async MFT e flags de stream output.
- T16: Apple Developer, ScreenCaptureKit, CoreVideo, VideoToolbox, ownership e availability; CGVirtualDisplay é API privada no código atual.
- T21: especificações de QUIC/Datagrams ou WebRTC/RTP, bibliotecas escolhidas e integração Android. Streams confiáveis e datagrams têm propriedades distintas.
- T24/T25: documentação Tauri de updater/assinatura, Android App Signing e políticas vigentes da loja. Aprovação da loja não se infere de um build.
- T30: portais freedesktop, PipeWire e APIs públicas dos compositores na versão testada.
- T31: VideoToolbox/Network framework, rede local e regras de background do iOS/iPadOS; não extrapolar possibilidades de USB do Android.
