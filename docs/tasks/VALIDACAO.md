# Matriz de validação e critérios de liberação

Usar apenas os cenários pertinentes à tarefa. Todos os cenários abaixo são planejados, não resultados já executados. Os resultados anteriores estão em EVIDENCIAS.md.

## Ambientes e registro

Para cada execução registrar commit, build debug/release, SO, CPU/GPU/driver, modelo/API Android, decoder/encoder reais, resolução/FPS, perfil, transporte e número de sessões. Evitar serial/IP/nome pessoal em artefato público.

| Dimensão | Cobertura desejada | Observação |
|---|---|---|
| Windows | 10/11 em versões explicitamente testadas; Intel/AMD/NVIDIA | Registrar suporte do SO à data da execução; não prometer suporte universal |
| Mac | Apple Silicon, macOS 14 e versões adicionais realmente disponíveis | API privada de monitor exige teste físico por versão |
| Android | API 30, 31/32, 33, 34, 35, 36 | Emulador cobre lifecycle/UX, não performance real do decoder |
| SoC | Ao menos MediaTek e Qualcomm, mais software fallback | Preferir aparelho onde o problema foi observado |
| Tela | Celular/tablet, retrato/paisagem, 60/90/120 quando disponível | Taxa configurada não prova refresh efetivo |
| Rede | Wi-Fi estável, degradado, Ethernet local, rede USB e ADB USB | ADB sem fio é um caso distinto |
| Monitores | Um/dois tablets, físico/virtual, múltiplas GPUs, DPI misto | Testes que mudam topologia precisam ambiente designado |
| Energia | Tela ligada/bloqueada, carregando/bateria, pausa e estado térmico | Não induzir aquecimento inseguro nem ignorar avisos do sistema |

Hardware indisponível fica como pendência nomeada. Não preencher uma célula como “passou” por analogia.

## Registro por release — v0.2.0 / 2026-09-29

Este registro separa execução local de cobertura ainda necessária:

| Alvo | Evidência desta execução | Estado honesto |
|---|---|---|
| Windows desktop | `cargo test --locked`: 76 unitários passaram, 1 foi ignorado por áudio físico e 1 integração passou; `node --check desktop/src/main.js`; host Windows, GPU/driver não caracterizados | testes locais passaram; captura/driver físico pendentes |
| Android JVM/build | Java 17; `testDebugUnitTest assembleDebug lintDebug assembleRelease`: BUILD SUCCESSFUL, 104 actionable tasks | passou no host; `local.properties` é arquivo local/ignorado |
| Android físico | Redmi Pad 2, API 36, MediaTek, release 0.2.0 via ADB bundled, USB reverse | APK atualizado instalado sem limpar dados; pareamento/reconexão, Surface/MediaCodec, T26 (menu/diálogo, IME e sentinela no Notepad), menu T27, T32/T33 release e reconexão USB `127.0.0.1` passaram; confirmação física bidirecional do conteúdo T27, clique desktop dos presets, writer/muxer T33 e matrizes adicionais pendentes |
| macOS | sem host macOS nesta sessão | apenas cobertura do workflow em runner permanece necessária |
| CI remoto | workflow inspecionado, não disparado | depende de execução autorizada no repositório |

O número de testes, hardware e obstáculos acima pertencem à execução datada; não são uma garantia para outras releases ou plataformas.

## Comandos de base

Na raiz desktop/src-tauri:

~~~powershell
cargo test --locked
cargo check --locked
~~~

Na raiz android:

~~~powershell
.\gradlew.bat testDebugUnitTest assembleDebug
.\gradlew.bat lintDebug
~~~

Comandos de lint/check adicionais devem existir na versão configurada; instalar ferramentas ou baixar dependências segue as permissões do ambiente. Testes instrumentados em dispositivo/emulador só após verificar o alvo e o escopo de instalação.

cargo fmt --check já falhava por formatação na base auditada. Registrar baseline, não misturar milhares de linhas de formatting com uma correção funcional. T23 define a adoção do check.

## Cenários funcionais

| ID | Preparação e ação | Resultado esperado | Tarefas principais |
|---|---|---|---|
| V01 | Handshake/HELLO válidos; controlar fragmentação de header/payload | Mesma sequência e framing nos dois clientes | T01, T02, T06 |
| V02 | Enviar versão antiga/nova e capability ausente/desconhecida | Fallback negociado ou erro claro antes de capturar | T02, T03 |
| V03 | Dimensões zero/gigantes, overflow, NaN/Inf, strings longas, tipo fora de estado | Rejeição limitada, sem panic/alocação gigante | T02, T34 |
| V04 | Peer autenticado para de ler; cancelar e consultar status | Encerramento em prazo, status e outras sessões responsivos | T06 |
| V05 | Peer envia bytes lentamente antes de HELLO/pareamento | Deadline total expira, resources liberados | T02, T06 |
| V06 | Parear A; beacon com mesmo pc_id aponta a certificado B | Token não enviado a B; confiança de A preservada | T11, T13 |
| V07 | Revogar device com sessão ativa; tentar reconectar com token antigo | Sessão perde autorização e antigo token é recusado | T11 |
| V08 | Conectar manualmente por IP, parear, fechar/reabrir e mudar IP | Identidade persistida permite achar PC sem pin inconsistente | T09, T11, T13 |
| V09 | 0/1/2 devices ADB, unauthorized/offline/emulador e ADB sem fio | Seleção explícita, transporte rotulado com evidência | T12 |
| V10 | Wi-Fi/USB/Ethernet simultâneos e PC sem acesso externo | Sessão local funciona; UI mostra endpoint realmente usado | T12, T13 |
| V11 | Remover cabo durante CONFIG, VIDEO, input e pausa | Erro único, liberação e retry conforme política | T06, T07, T09 |
| V12 | Decoder A aceita modo; decoder B não; selecionar modo 90/120 | Modo usa decoder correto ou redução explícita | T03 |
| V13 | Monitor 4K em Mirror; escolher Performance e Custom menor | Stream reduz pixels, monitor físico não muda | T04 |
| V14 | Custos simulados de frame e deadlines perdidos | Sem intervalo somado duas vezes nem rajadas atrasadas | T04 |
| V15 | Cena estática, mover cursor uma vez ou digitar um caractere | Último update aparece e app retorna a estado ocioso | T04, T07, T20 |
| V16 | Callback antigo chega após CONFIG/release/novo decoder | Índice/estado antigo ignorado, sem crash | T07 |
| V17 | Rotacionar, bloquear/desbloquear e recriar Activity 30 vezes | Uma sessão, sem crescimento de recursos ou tela preta permanente | T07, T09 |
| V18 | Segundo plano prolongado e timeout de serviço aplicável | Política/cleanup corretos; notificação coerente | T09, T10 |
| V19 | Negar notificações, permissão Mac de input/captura ou acesso ao dispositivo | Erro/limitação pertinente sem ciclo infinito | T09, T16, T22 |
| V20 | Pausar A enquanto B segue; retomar A | Sem backlog/alteração do monitor B; retoma por IDR | T10, T18 |
| V21 | Congestionar rede, encoder e decoder separadamente | Diagnóstico identifica causa; filas têm limites | T05, T20 |
| V22 | Descartar frame/referência e pedir IDR | CONFIG/parâmetros/geração coerentes, recuperação válida | T07, T20 |
| V23 | Arrastar/caneta e desconectar; alternar ordem de dedos | Nenhum botão preso; contato correto controla mouse | T08 |
| V24 | Toque em cantos/centro com letterbox, rotação e DPI misto | Coordenadas corretas, sem clique em barras locais | T08 |
| V25 | Duas sessões, mudança de perfil em A; bandeja altera modo | B não é reconfigurada indevidamente; UI reflete backend | T18 |
| V26 | Remover monitor escolhido e provocar falha de virtual display | Outra tela não é transmitida sem política explícita | T19 |
| V27 | Reaplicar o mesmo modo com dois tablets e depois girar um | Sem loop de rebuild; mudança coordenada quando necessária | T19 |
| V28 | Gerar modos inválidos/novos em fake de serviço e XML | Limites/rejeição, sem expansão ilimitada de modos | T19, T34 |
| V29 | Som/silêncio/mute, saída alterada e write parcial | Sem áudio velho, vazamento nem derrubar vídeo | T17 |
| V30 | Instalação limpa, upgrade, erro de serviço/disco e rollback | Estado real informado, dados e driver alheio preservados | T24 |
| V31 | Alterar assinatura/manifesto/pacote de update em servidor de teste | Recusa verificável, nunca executa payload adulterado | T24 |
| V32 | Falha de escrita/corrupção de Settings, certificado e pareamentos | Sem sucesso falso, perda silenciosa de identidade ou panic | T11, T18 |
| V33 | Fontes ampliadas, TalkBack, split-screen, tela pequena | Controles e saída da sessão continuam acessíveis | T22 |
| V34 | Payloads com segredos-sentinela em diagnóstico | Ausentes de logs/telemetria/exportação não autorizada | T05, T25 |

## Benchmark de desempenho

Executar em release, com o mesmo hardware, cena e build flags. Não comparar debug antigo com release novo.

1. Registrar o modo e confirmar encoder/decoder efetivos.
2. Aquecer por 30 segundos e medir por 60 segundos; repetir três vezes. Informar variação e condições térmicas.
3. Usar cenas: estática, scroll de texto, movimento contínuo e updates isolados.
4. Registrar FPS de captura/encode/envio/render observado, intervalos p50/p95/p99, quedas, bitrate, filas em frames/bytes/idade, CPU e memória/VRAM quando acessível.
5. Medir RTT e durações locais separadamente. Latência visual precisa método externo ou estimativa explicitamente identificada.
6. Testar um e dois tablets; Wi-Fi e USB com mesma resolução/bitrate.
7. Avaliar qualidade visual de texto, cursor, gradientes e cores para impedir “ganho de FPS” conseguido por regressão invisível nas configurações.
8. Medir instrumentação ligada/desligada para verificar sobrecarga.

Metas propostas, a calibrar e registrar antes de cada otimização:

- Sem crescimento ilimitado de memória ou idade de fila.
- Em hardware capaz, animação contínua aproxima-se do FPS negociado; exigir resultado observado, não 60 universais.
- p95 de latência e uso de CPU não pioram de forma material sem tradeoff justificado.
- Recuperação de curto congestionamento volta a conteúdo recente; o alvo inicial de T07/T20 é dois segundos no ambiente de referência.
- Não declarar ganho se o cenário comparado mudou resolução, codec, taxa da tela, modo de energia ou qualidade.

## Teste prolongado

Após correções de transporte/lifecycle, executar sessão de duas horas em ambiente designado, com mudanças periódicas de perfil/orientação/áudio e um conjunto de 100 reconnects automatizados no harness.

Observar threads, handles, RSS/VRAM, bateria/estado térmico, timeouts, input preso, áudio acumulado e monitores órfãos. Teste de limites longos de foreground service segue documentação atual e ambiente Android próprio; duas horas não provam ausência de falha no limite de seis horas.

## Aceite de uma release

- Casos pertinentes às mudanças passam; pendências físicas e plataformas não testadas estão explícitas.
- Sem perda de autenticação, exposição de monitor diferente, input preso ou regressão de updater.
- Migrações de configuração/identidade e combinação desktop/Android possuem teste.
- Assinaturas e binários são verificáveis; nenhuma chave debug acidental.
- Plano de retorno e limitações são entregues ao usuário; publicar continua sendo ação separada.

Documentação deste backlog: verificar links relativos, IDs únicos, dependências sem ciclos e correspondência entre índice e tarefas. Não é necessário recompilar o aplicativo só porque este arquivo foi editado.
