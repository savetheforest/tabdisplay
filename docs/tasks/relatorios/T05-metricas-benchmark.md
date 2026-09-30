# T05 — Métricas e benchmark reproduzível

## Identificação

- ID e título: T05 — Medir cada etapa e produzir benchmarks comparáveis
- Status: Em validação
- Base: `d823e5b` + T01/T02/T03/T04/T06 parcial + working tree de 29/09/2026
- Dependências verificadas: T01/T02 concluídas; T03/T04/T06 ainda aguardam parte da validação física/adversarial

## Resultado

- `Stats` agora diferencia FPS enviado, FPS único (captura nova), repetições FLUSH, submissões ao decoder, render observado pelo callback de saída e descartes por falta de input buffer.
- A janela de métricas usa no máximo 128 amostras por série e calcula p50/p95/p99 localmente; não guarda payload, frame, conteúdo de tela ou log ilimitado.
- Captura, encode e escrita TLS têm tempos locais separados. O relógio é `Instant` no PC; nenhum timestamp do PC é comparado diretamente ao relógio do tablet.
- `config_id` monotônico foi adicionado como campo opcional de CONFIG/STATS v3. O vídeo binário não mudou. O callback Android verifica a instância atual do codec e só conta render quando `releaseOutputBuffer` realmente retorna sucesso.
- Tela estática é marcada como `idle` quando não chega nova captura; repetições provisórias do FLUSH não são classificadas como atividade de rede.
- O painel existente passou a mostrar codec, encoder, alvo, decode/render observados, p95 de encode e separação entre frames únicos e FLUSH. O botão de diagnóstico exporta JSON local sanitizado em Downloads, sem nome do tablet, IP, token, serial ou conteúdo.

## Arquivos e decisões

- `desktop/src-tauri/src/server.rs`: `Stats`, `MetricWindow`, contadores e percentis; captura/encode/envio instrumentados; relatório sanitizado.
- `android/app/src/main/java/com/tabdisplay/Stream.kt`: contadores de decode/render, falhas, descartes, `config_id` e guarda contra callback de codec antigo.
- `desktop/src/main.js`, `desktop/src/index.html`, `desktop/src-tauri/src/lib.rs`: painel e exportação local explícita.
- `PROTOCOL.md`: contrato opcional de `config_id` e STATS detalhado, sem mudança do envelope ou da access unit H.264.

## Validação

| Verificação | Comando/ambiente | Resultado |
|---|---|---|
| Janela limitada e percentis | `cargo test --locked` em Windows | Passou: teste cobre 128 amostras, 20 descartes e p50/p99; suíte atual: 45 unitários, 1 ignorado, 1 integração |
| Integração do relatório sanitizado | inspeção de `metrics_report` e comando Tauri | Passou por revisão estática: somente `Stats` agregadas; nenhum campo de sessão, endereço, token, nome ou serial é serializado |
| Cliente Android | `android\gradlew.bat testDebugUnitTest assembleDebug` | Passou; build Gradle 9.7.1 concluído |
| Falha de renderização e callback antigo | revisão + compilação JVM | Passou por guarda de instância e contagem somente após `releaseOutputBuffer` bem-sucedido; callback de Surface real ainda não reproduzido |
| Benchmark físico comparável | release workspace, USB/ADB reverse, Performance, 1152×640, alvo 60 fps/10 Mbps, aquecimento 30 s por repetição | Passou parcialmente: cena estática, três repetições; R1 60 amostras: 59,0 fps médios, 13,4 ms RTT médio, 9,81 Mbps; R2 30: 54,8 fps, 12,0 ms, 9,15 Mbps; R3 30: 48,7 fps, 11,8 ms, 8,11 Mbps. Sessão permaneceu `127.0.0.1↔127.0.0.1`; temperatura observada ao final: bateria 38 °C, pele 41,3 °C, status térmico normal. |

## Roteiro reproduzível pendente

Executado para cena estática em release workspace no Redmi Pad 2, com os mesmos parâmetros e transporte USB. Ainda falta repetir três vezes para scroll/texto e movimento, comparar instrumentação ligada/desligada e exportar o JSON sanitizado pelo painel. A variação entre repetições não é atribuída a uma causa sem controlar energia/temperatura; RTT continua separado de latência visual.

## Reversão e limitações

Os campos são opcionais e clientes legados recebem defaults seguros. Para voltar ao comportamento anterior, remova a coleta `MetricWindow` e use apenas os contadores essenciais; o protocolo VIDEO e os dados persistidos não precisam de migração. Render observado pelo callback não equivale a emissão de luz no painel; latência visual real ainda exige cena de referência e medição externa.
