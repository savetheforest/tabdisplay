# T17 — Áudio, filas e sincronização

## Identificação

- ID e título: T17 — Manter áudio recente, estável e sincronizado
- Status: Em validação
- Base: `d823e5b` + T01–T16 parciais + working tree de 29/09/2026
- Plataformas compiladas: Windows desktop e Android debug; macOS não disponível

## Resultado

O áudio do PC agora usa uma fila de 8 frames de 20 ms, ou 160 ms de orçamento.
Quando o escritor fica atrás, os pacotes mais antigos são descartados para
recuperar conteúdo recente; a fila não acumula ~500 ms por desenho. Mute,
áudio desligado, pausa e retomada limpam os pacotes pendentes.

Clientes Android novos anunciam `audio_timestamps:true` no HELLO. Para eles o
PC envia a extensão v3 `[version=1][sequence:u64][pts_samples:u64][Opus]`;
clientes antigos continuam recebendo Opus puro. O PTS é relativo ao começo da
captura em amostras de 48 kHz, não é relógio de parede nem latência visual.

No Android, o player:

- reconhece a extensão e preserva fallback para payload legado;
- usa `info.offset`/`info.size` do MediaCodec;
- trata `INFO_OUTPUT_FORMAT_CHANGED` e buffers ausentes;
- repete `AudioTrack.write` não bloqueante apenas para o remanescente que couber,
  sem esperar preencher todo o buffer;
- detecta lacuna de sequência e limpa decoder/AudioTrack antes de continuar;
- faz flush ao mutar ou liberar, sem alterar volume do PC.

## Arquivos e decisões

- `desktop/src-tauri/src/audio.rs`: `Packet`, `Queue`, orçamento temporal e
  descarte de antigos.
- `desktop/src-tauri/src/server.rs`: capacidade `audio_timestamps`, serialização
  versionada e limpeza durante PAUSE/mute.
- `android/.../Audio.kt`: parsing, PTS, lacuna, escrita parcial e flush.
- `android/.../MainActivity.kt`/`Stream.kt`: negociação e consumo.
- `PROTOCOL.md`: contrato e compatibilidade.

Captura continua uma origem por sessão. Compartilhar loopback entre tablets não
foi introduzido: ownership e medição de duplicação ainda não estão demonstrados.

## Validação

| Verificação/cenário | Ambiente e comando | Resultado observado | Artefato |
|---|---|---|---|
| Opus e fila temporal | `cargo test --locked` em Windows | Passou: 45 unitários, 1 ignorado, 1 integração; teste confirma drop-oldest em 160 ms | `audio.rs` |
| Parsing de AUDIO v1 e legado | `gradlew.bat testDebugUnitTest assembleDebug` | BUILD SUCCESSFUL; testes Android incluem 2 casos de sequência/PTS | `AudioTest.kt` |
| Áudio Windows real, escrita e troca de dispositivo | loopback físico | Não executado; teste WASAPI existente continua ignorado sem áudio real | pendente |
| Áudio macOS/Bluetooth/rota e sleep/wake | Mac real | Não executado | pendente |
| Tom, silêncio, perda, pausa e clique audiovisual | tablet/saída designados | Não executado; não instalar/abrir APK por bloqueio de mutação do tablet | pendente |

Não foram inventados números de latência audiovisual, consumo ou sincronização
A/V. PTS/sequence permitem detectar perda e limpar backlog, mas não constituem
uma medição de alinhamento visual entre relógios diferentes.

## Operação e reversão

O envio pode continuar desativado na configuração existente. Para rollback,
manter `Queue::clear` e retirar apenas a extensão anunciada/serializada; o PC e
o tablet voltam ao payload Opus legado sem tocar em pareamentos ou dados.

Próximo passo exato: testar uma sessão release com tom/silêncio e escrita
parcial em Windows e Android, depois medir A/V com o mesmo dispositivo de saída;
se o caminho passar, repetir em macOS e Bluetooth identificando o atraso próprio
da rota.
