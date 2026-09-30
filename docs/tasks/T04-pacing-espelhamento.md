# T04 — Corrigir temporização e aplicar a resolução escolhida ao espelhamento

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T01, T03. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Problemas confirmados

Em server.rs:stream_once, last é atualizado depois de encode/write. Assim o próximo frame espera um intervalo inteiro além do trabalho já feito. Exemplo teórico: 16,7 ms de intervalo + 8 ms de trabalho dá no máximo ~40,5 FPS, antes de outros custos; não é um benchmark real.

plan calcula w/h para Performance/Custom, mas open_capture recebe tablet. No espelhamento, a captura pode continuar no tamanho máximo mesmo após escolher resolução menor.

## Implementação

1. Separar decisão de agendamento de captura/codificação e testá-la com relógio falso.
2. Usar deadlines monotônicos de apresentação/envio desejados. Definir se o próximo deadline avança a partir do anterior ou do instante de início, sem acrescentar novamente encode/write.
3. Quando atrasar, saltar deadlines vencidos e usar o frame bruto mais recente; nunca tentar recuperar o atraso com uma rajada de frames velhos.
4. Preservar último update de uma sequência e a primeira imagem. Manter timeout de captura suficiente para processar controle/áudio.
5. Não codificar buffer vazio se não houve primeira captura válida. Avaliar o FLUSH de 250 ms por dispositivo; não removê-lo sem provar que updates isolados continuam visíveis.
6. Passar limite efetivo do perfil e da negociação à captura. No Mirror, conservar proporção do monitor fonte, dentro do envelope escolhido; nunca distorcer nem alterar a resolução do monitor físico.
7. Distinguir resolução do desktop, saída do vídeo e coordenadas de input. O toque continua mapeado ao rect original e ao viewport.
8. No Extend, manter a relação entre modo virtual e stream documentada. Não reiniciar driver por cada deadline.
9. Atualizar CONFIG, status e textos de perfil com valores efetivamente aplicados.

## Critérios de aceite

- [ ] Com custo simulado menor que o orçamento, 60 FPS não vira 1/(intervalo + custo).
- [ ] Sob custo maior que o orçamento, a fila não cresce e não há rajadas para “compensar”.
- [ ] Performance reduz pixels no Mirror; Custom respeita limite selecionado.
- [ ] Cursor/tecla isolados chegam ao tablet quando a cena fica estática.
- [ ] Input acerta os quatro cantos e centro após redução/letterbox.
- [ ] CPU ociosa não fica em busy loop com timeout zero.

## Validação

Testar 30/60/90/120 com custos falsos 0, 5, 15, 25 e 50 ms; resize/rebuild no meio de uma espera; fonte 4K para saídas menores; fonte retrato; cena estática seguida de um update.

Comparar em release no mesmo hardware. Sem T05 concluída, registrar medições provisórias identificadas; T05 fornecerá detalhamento de etapas, não é pré-requisito circular desta correção.

Rollback: restaurar agendador via mudança isolada preservando limites de capacidade; manter a correção da resolução independente para permitir reversão parcial.
