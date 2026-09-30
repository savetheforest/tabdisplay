# T33 — Gravar a sessão localmente sob ação explícita

Prioridade: P3. Tipo: Opcional. Status: Em validação.
Dependências: T02, T05, T17, T25.

## Decisão inicial

MVP sugerido: gravar no PC o vídeo já codificado enviado a uma sessão, com áudio opcional e destino escolhido. Confirmar no escopo de execução se a gravação deve ficar no tablet; não implementar os dois lados simultaneamente sem necessidade.

## Implementação

Núcleo de contrato adicionado em `desktop/src-tauri/src/recording.rs`:

- contêiner escolhido: Matroska para H.264 + Opus timestampados;
- espera de configuração e primeiro IDR antes de aceitar vídeo;
- timestamps de vídeo/áudio monotônicos e limite de duração de 2 horas;
- resize/codec generation exige novo segmento e novo IDR;
- fila bounded de 16 MiB, pacotes de até 4 MiB e retorno explícito de fila cheia;
- pausa, mute, cancelamento e descarte até IDR não bloqueiam o streaming nem
  alteram um segmento já drenado.
- ciclo de destino adicionado: valida o pai e o nome final, recusa sobrescrita,
  cria temporário exclusivo identificado no mesmo diretório, e expõe commit,
  cancelamento e limpeza limitada ao temporário criado.
- muxer puro adicionado em `desktop/src-tauri/src/matroska.rs`: escreve
  cabeçalho EBML, Info/Tracks AVC+Opus, CodecPrivate AVC, conversão de
  access-unit Annex-B para length-prefixed e clusters com timecode; rejeita
  segmento sem IDR/SPS/PPS ou timestamps inválidos.
- ponte explícita `TempRecording::write_matroska_segment` adicionada para
  escrever um segmento muxado no temporário próprio; ela não inicia gravação
  nem decide quando a sessão deve drenar a fila.
- `SegmentWriter` adicionado com canal `sync_channel` de dois segmentos:
  `submit` usa `try_send`, o worker faz o I/O em thread própria e cada segmento
  recebe temporário/commit independente. Fila cheia, canal fechado e panic do
  worker têm erros explícitos; rotação por configuração não concatena
  contêineres incompatíveis.

O ciclo de destino faz apenas a guarda de arquivo e I/O mínimo de temporário.
O muxer é puro e determinístico, mas ainda não está ligado ao `Recorder`, à
fila da sessão ou ao destino; portanto o produto ainda não declara uma
gravação reproduzível. O writer assíncrono está conectado à saída codificada
por um controlador por sessão: CONFIG abre a geração, vídeo/áudio são
duplicados para a fila bounded e resize/generation drena o segmento anterior e
abre um arquivo irmão. Indicador persistente, leitor independente e validação
física do fluxo ainda continuam pendentes.

1. Definir contêiner, codecs aceitos, timestamps e limites de duração/tamanho. Annex-B e pacotes Opus não podem ser concatenados como se fossem um MP4 válido.
2. Iniciar com IDR/configuração suficiente para arquivo decodificável. Preservar relógio de mídia e sincronização T17.
3. Decidir segmentação ou reconfiguração de arquivo quando resolução/codec mudar; não produzir trilha inválida após rotação.
4. Duplicar a saída codificada para fila limitada de escrita. Disco lento ou fila cheia encerra somente a gravação com erro visível, sem travar streaming/input.
5. Mostrar indicador persistente e botão de parar; desconexão/fechamento finalizam contêiner ou deixam recuperação documentada.
6. Solicitar destino, impedir sobrescrita silenciosa e tratar disco cheio. Temporários ficam identificados e cleanup afeta só arquivos criados pela gravação. A guarda de destino/temporário e o commit assíncrono de segmentos já estão cobertos; disco cheio ainda depende da integração.
7. Não incluir gravação em diagnósticos/Sentry nem iniciar automaticamente por reconexão.
8. Registrar limites de captura de conteúdo protegido e não tentar contornar proteção do SO.
9. Atualizar privacidade e UX com local, finalidade e exclusão manual.

## Aceite e testes

- [ ] Arquivo final é reproduzível por leitor independente.
- [ ] A/V sincroniza e troca de configuração não corrompe arquivo.
- [ ] Parar/cancelar/disco cheio têm estado claro e não afetam arquivos existentes.
- [x] O contrato de fila é bounded e retorna fila cheia sem bloquear o stream.
- [ ] Overhead no stream é medido e limitado.
- [ ] Sessão/usuário sabe que a gravação está ativa.

Testar início no meio de GOP, resize, mute, pausa, queda, arquivo zero/curto e processo interrompido. Usar cenas fictícias.

Rollback desativa feature e preserva gravações existentes; nunca apagar mídia pessoal para limpar a funcionalidade.
