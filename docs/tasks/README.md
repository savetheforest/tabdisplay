# Backlog técnico do TabDisplay

Data: 29/09/2026. Base inspecionada: commit d823e5b, versão 0.2.0, protocolo v3.

Este backlog documenta trabalho futuro. Nenhuma tarefa está implementada por sua presença aqui. O pedido que originou estes documentos autoriza a documentação; execução, publicação e mudanças de produto devem seguir o pedido vigente quando uma tarefa for selecionada.

## Como executar

1. Leia [GUIA-EXECUCAO.md](GUIA-EXECUCAO.md), [EVIDENCIAS.md](EVIDENCIAS.md) e a tarefa escolhida.
2. Confira suas dependências na tabela; releia o código, pois a base pode ter mudado.
3. Implemente uma tarefa ou uma etapa explicitamente delimitada. Atualize o status e registre a evidência de validação.
4. Use [VALIDACAO.md](VALIDACAO.md) para selecionar cenários; não declare hardware não testado como compatível.
5. Use [TEMPLATE-ENTREGA.md](TEMPLATE-ENTREGA.md) no relatório de entrega.

Prioridade: P0 = correção de fronteiras de confiança/recursos; P1 = funcionamento e estabilidade; P2 = desempenho e experiência; P3 = expansão opcional. P0 não significa que exista exploração reproduzida. Dependências são pré-requisitos de integração; leitura e investigação podem começar antes.

Tipos: **Core** melhora funções existentes; **Opcional** amplia produto; **Spike** produz uma decisão fundamentada e, se necessário, um protótipo isolado. Um spike pode terminar com a recomendação de não implementar.

## Tarefas

| ID | Tarefa | Prioridade | Tipo | Depende de | Status |
|---|---|---|---|---|---|
| T01 | [Base de testes e fixtures](T01-base-testes.md) | P1 | Core | — | Concluída |
| T02 | [Contrato de protocolo e validação](T02-protocolo.md) | P0 | Core | T01 | Concluída |
| T03 | [Negociação de decoder, resolução e FPS](T03-capacidades-video.md) | P1 | Core | T02 | Em validação |
| T04 | [Temporização e resolução no espelhamento](T04-pacing-espelhamento.md) | P1 | Core | T01, T03 | Em validação |
| T05 | [Métricas e benchmark reproduzível](T05-metricas-benchmark.md) | P1 | Core | T01, T02, T03 | Em validação |
| T06 | [TLS, cancelamento e conexões lentas](T06-tls-concorrencia.md) | P0 | Core | T01, T02 | Em validação |
| T07 | [Decoder Android e recuperação de vídeo](T07-decoder-android.md) | P1 | Core | T02, T03, T06 | Em validação |
| T08 | [Toque, caneta e liberação de entrada](T08-entrada.md) | P1 | Core | T02, T07 | Em validação |
| T09 | [Sessão Android e reconexão](T09-sessao-android.md) | P1 | Core | T01, T06, T07 | Em validação |
| T10 | [Pausa, energia e limites térmicos](T10-energia.md) | P2 | Core | T02, T05, T09 | Em validação |
| T11 | [Identidade, pareamento e revogação](T11-identidade-pareamento.md) | P0 | Core | T01, T02, T06 | Em validação |
| T12 | [USB ADB com múltiplos aparelhos](T12-usb-adb.md) | P1 | Core | T01, T11 | Em validação |
| T13 | [Descoberta, interfaces e operação offline](T13-rede-offline.md) | P1 | Core | T01, T11, T12 | Em validação |
| T14 | [USB direto via AOA: viabilidade](T14-usb-aoa-spike.md) | P3 | Spike | T02, T05, T11, T12, T13 | Concluída |
| T15 | [Pipeline GPU no Windows](T15-gpu-windows.md) | P2 | Core | T03, T04, T05, T07 | Em validação |
| T16 | [Pipeline e compatibilidade macOS](T16-pipeline-macos.md) | P2 | Core | T03, T04, T05, T07 | Em validação |
| T17 | [Áudio, filas e sincronização](T17-audio.md) | P1 | Core | T02, T05, T06, T09 | Em validação |
| T18 | [Configurações por tablet e persistência](T18-configuracoes-sessoes.md) | P1 | Core | T01, T02, T11 | Em validação |
| T19 | [Monitores, driver e recuperação](T19-monitores-driver.md) | P1 | Core | T04, T18, T34 | Em validação |
| T20 | [Controle de congestionamento e perfil automático](T20-filas-adaptacao.md) | P2 | Core | T03, T04, T05, T06, T07, T18 | Em validação |
| T21 | [Transporte de mídia: decisão e protótipo](T21-transporte-spike.md) | P3 | Spike | T05, T06, T11, T17, T20 | Em validação |
| T22 | [Experiência mobile e diagnóstico](T22-mobile-ux.md) | P2 | Core | T03, T05, T09, T12, T13, T18 | Em validação |
| T23 | [CI, integração e matriz de compatibilidade](T23-ci-qualidade.md) | P1 | Core | T01 | Em validação |
| T24 | [Instaladores, assinatura e atualizações](T24-distribuicao.md) | P1 | Core | T11, T19, T23, T34 | Em validação |
| T25 | [Privacidade, documentação e produto](T25-privacidade-produto.md) | P1 | Core | T05, T11, T22, T24 | Em validação |
| T26 | [Teclado, trackpad e atalhos](T26-teclado-atalhos.md) | P3 | Opcional | T02, T08, T09, T11, T22 | Em validação |
| T27 | [Área de transferência](T27-clipboard.md) | P3 | Opcional | T02, T06, T09, T11 | Em validação |
| T28 | [Transferência local de arquivos](T28-arquivos.md) | P3 | Opcional | T02, T06, T11, T20 | Em validação |
| T29 | [HEVC/AV1 e qualidade de imagem](T29-codecs-spike.md) | P3 | Spike | T03, T05, T07, T15, T16 | Concluída |
| T30 | [Linux por etapas](T30-linux-spike.md) | P3 | Spike | T01, T02, T05, T06, T08, T17, T23 | Concluída |
| T31 | [Cliente iOS/iPadOS: viabilidade](T31-ios-spike.md) | P3 | Spike | T02, T03, T05, T11, T13 | Concluída |
| T32 | [Layouts e modo apresentação](T32-layouts-apresentacao.md) | P3 | Opcional | T10, T18, T19, T22 | Em validação |
| T33 | [Gravação local da sessão](T33-gravacao.md) | P3 | Opcional | T02, T05, T17, T25 | Em validação |
| T34 | [Serviço privilegiado e limites de recursos](T34-servico-seguranca.md) | P0 | Core | T01, T02 | Em validação |

## Sequência sugerida

- Fundação: T01 → T02; então T06, T11 e T34. T23 pode evoluir junto da fundação.
- Correções perceptíveis: T03 → T04 → T05 → T07; depois T08 e T09.
- Conexão e múltiplos aparelhos: T12 → T13, e T18 → T19.
- Experiência e desempenho: T17, T10, T20, T22; depois T15 e T16 com números comparáveis.
- Preparação de distribuição: T24 → T25, usando a matriz de T23.
- Novos recursos: selecionar os opcionais e spikes conforme demanda. Não executar todos automaticamente.

Esta é uma sugestão de ordem; a coluna de dependências prevalece. O pacote não prevê uma reescrita total, troca de framework, servidor na nuvem ou mudança de monetização.

## Coordenação entre agentes

server.rs, Stream.kt, MainActivity.kt, PROTOCOL.md e settings.rs são arquivos de disputa frequente. Não atribuir alterações simultâneas neles a agentes independentes sem definir interfaces e um integrador. Uma tarefa que altera o contrato deve integrar ambos os lados antes das tarefas consumidoras.

T04 corrige o agendamento existente; T20 controla filas/adaptação; T15/T16 otimizam implementações de plataforma; T21 avalia substituir transporte. Cada uma deve reutilizar os contratos das anteriores.

## Referências

- [Evidências e correções da análise](EVIDENCIAS.md)
- [Regras de execução](GUIA-EXECUCAO.md)
- [Matriz de validação](VALIDACAO.md)
- [Fontes técnicas oficiais](REFERENCIAS.md)
- [Modelo de entrega](TEMPLATE-ENTREGA.md)
