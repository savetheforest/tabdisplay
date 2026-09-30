# Relatório T33 — Gravação local

Data: 29/09/2026  
Status: Em validação

Foi adicionado o núcleo bounded `desktop/src-tauri/src/recording.rs`; o
`SegmentWriter` faz I/O assíncrono de segmentos completos em temporários
exclusivos e o servidor o chama somente após ação explícita. A decisão é Matroska
para H.264 + Opus com timestamps; o estado espera CONFIG/IDR, rejeita timestamps
regressivos, exige novo segmento após resize/generation, limita a fila a 16
MiB e cada pacote a 4 MiB, impõe 2 horas e trata pausa, mute, fila cheia e
cancelamento.

O primeiro MVP continua sendo no PC, após ação explícita, duplicando a saída codificada para arquivo em destino escolhido; não gravar no tablet e no PC simultaneamente.

Além dos quatro testes do núcleo, três testes de destino passaram: recusa de
sobrescrita e commit do temporário criado, cancelamento/Drop sem apagar outro
arquivo, e validação de pai/nome final. O ciclo cria apenas um `.part` exclusivo
no mesmo diretório e faz commit sem substituir um destino já existente. Isso é
apenas guarda de arquivo; ainda não prova que o conteúdo seja reproduzível.

O release foi recompilado com o núcleo e reiniciado. A sessão do Redmi Pad 2 foi revalidada por USB após desligar temporariamente o Wi‑Fi: `127.0.0.1:7070 ↔ 127.0.0.1:62610`, SurfaceView ativo a `45 fps · 8 ms · 7,5 Mbps`; o Wi‑Fi foi restaurado e nenhum dado foi removido. Isso é regressão da sessão, não aprovação do muxer.

O muxer puro `desktop/src-tauri/src/matroska.rs` foi adicionado: gera EBML
Matroska com trilha H.264/AVC (CodecPrivate derivado de SPS/PPS), trilha Opus,
clusters com timecodes em milissegundos e conversão dos access units Annex-B
para length-prefixed. Ele rejeita segmento vazio, início sem IDR/SPS/PPS,
timestamps regressivos, áudio inesperado, pacotes acima do limite e duração
acima de duas horas. Os três testes do muxer passaram; a suíte desktop global
agora soma 78 testes unitários, 1 ignorado e 1 integração. Isso ainda não prova
reprodução independente nem integra o writer à sessão.

O muxer também rejeita uma fatia de packets acima do orçamento bounded de 16
MiB antes de alocar a saída; esse cenário passou em teste junto dos limites de
IDR/SPS/PPS e timestamps.

A ponte `TempRecording::write_matroska_segment` agora liga o muxer puro ao
ciclo de temporário, com erro separado para falha de mux ou destino. Um teste
confirma que um segmento muxado começa no temporário pertencente à gravação e
só chega ao caminho final após `commit`. Também foi adicionado
`SegmentWriter`: um worker assíncrono com `sync_channel` de dois segmentos,
`try_send` sem espera no chamador e commit independente por destino. O teste
do writer confirma que o arquivo final é criado somente após o worker terminar.
O servidor agora chama o controlador por sessão: CONFIG define a geração,
VIDEO/AUDIO são duplicados para o `Recorder`, e uma rotação drena o segmento
anterior para um arquivo irmão (`.001.mkv`, etc.). Um teste de integração do
controlador verifica dois contêineres independentes após mudança de geração.

Após a guarda contra destinos existentes, o release final foi recompilado e
relançado como PID 31680. Com Wi‑Fi temporariamente desligado, a seleção
“Conexão local pelo ADB” confirmou `127.0.0.1:7070 ↔ 127.0.0.1:57264`, com
`MainActivity`/`SurfaceView` ativos; o Wi‑Fi foi restaurado e nenhum dado foi
apagado. O writer incremental continua não integrado; o muxer puro não foi
 contado como reprodução independente.

Após a ponte explícita do writer, o release final foi recompilado e relançado
como PID 6552. A sessão USB confirmou `127.0.0.1:7070 ↔ 127.0.0.1:60716`, com
`MainActivity`/`SurfaceView` ativos; o Wi‑Fi foi restaurado e nenhum dado foi
apagado. O método de escrita de segmento ainda não é chamado pela sessão.

Após o limite bounded de 16 MiB do muxer, o release final foi recompilado e
relançado como PID 31380. A sessão USB confirmou `127.0.0.1:7070 ↔
127.0.0.1:54089`, com `MainActivity`/`SurfaceView` ativos; o Wi‑Fi foi
restaurado e nenhum dado foi apagado.

Em seguida foi gerado um novo instalador NSIS 0.2.0 a partir do release atual,
SHA-256 `6BF95AB01EAE376F77066CF3580F75C33846E8F65D25D29160DF0488E2587DE0`.
As duas tentativas de instalação elevada (`Start-Process -Verb RunAs`, uma
interativa e uma silenciosa) não receberam consentimento UAC: o executável em
`C:\Program Files\TabDisplay` permaneceu com SHA-256
`5C9DEFD05BF9F170827DB44826061E58EF9BEC64DDFAFB1FF41157CEEF210E3B`.
O release do workspace foi relançado como PID 18212; com Wi‑Fi desligado, o
tablet foi selecionado pelo item “Conexão local pelo ADB” e confirmou
`127.0.0.1:7070 ↔ 127.0.0.1:61762`, `MainActivity` ativa; o Wi‑Fi foi
reativado e os dados foram preservados.

Depois da inclusão do `SegmentWriter`, `cargo build --locked --release` passou
e o release foi relançado como PID 31384 (SHA-256
`0B7B64DC3BF535385A2707663204AEB637425B29F9FD29A13B52AF09E590D5EE`). Com o
Wi‑Fi temporariamente desligado, a seleção “Conexão local pelo ADB” confirmou
`127.0.0.1:7070 ↔ 127.0.0.1:65485`; o Wi‑Fi foi restaurado e os dados foram
preservados.

Após integrar o controlador por sessão, `cargo build --locked --release`
passou e o release foi relançado como PID 27816 (SHA-256
`8127BEF39BD09A64FDA1F0D42AD86913C797F2A9C1564700495B968A58CDF140`). Com o
Wi‑Fi temporariamente desligado, a seleção “Conexão local pelo ADB” confirmou
`127.0.0.1:7070 ↔ 127.0.0.1:49421`; `MainActivity` permaneceu ativa, o Wi‑Fi
foi restaurado e os dados foram preservados.

O bundle final, já com o controlador e os comandos/UI, foi gerado em
`desktop/src-tauri/target/release/bundle/nsis/TabDisplay_0.2.0_x64-setup.exe`,
SHA-256 `06E6CCA707237291C6148A0D14B4D346922C83532E556741CF8A449D625962CE`.
A instalação silenciosa elevada retornou código 2; o executável em
`C:\Program Files\TabDisplay` continuou sem alteração por falta de elevação
UAC. O release foi relançado como PID 1488 (SHA-256
`7DBAA5E77F806F589EDE64D42077CC8EB73D9FCC31DE889F3D61BB7CC835D3FD`); usando
explicitamente o serial físico `kfxkfin7nbpfq4q8`, o reverse USB permaneceu
ativo em `127.0.0.1:7070 ↔ 127.0.0.1:53780`, `MainActivity` ficou ativa e o
Wi‑Fi foi restaurado.

Para concluir o T33, é necessário validar os arquivos com leitor independente,
exercitar início/parada pelo novo controle, A/V com áudio T17, disco cheio,
queda/retomada e limites de overhead. A integração já garante que a escrita não
é feita no thread de vídeo/input e que resize/CONFIG não concatena segmentos
incompatíveis. O arquivo
não pode concatenar Annex-B e Opus como MP4 inválido; o muxer puro já evita
essa decisão errada, mas ainda não foi ligado à fila/sessão.

Pendentes: UX de indicador/parar, leitor independente, A/V, crash/disco cheio, proteção de conteúdo e atualização de privacidade. Nenhuma gravação começa por reconexão ou diagnóstico.
