# T29 — Avaliar HEVC/AV1 e melhorar nitidez sem comprometer latência

Prioridade: P3. Tipo: Spike opcional. Status: Concluída.
Dependências: T03, T05, T07, T15, T16.

## Pergunta

HEVC/AV1 melhora legibilidade/bitrate/consumo nos aparelhos de interesse, mantendo encode/decode de baixa latência? Texto fino e subamostragem de cor também devem ser avaliados; mais bitrate não resolve todas as perdas de qualidade.

## Trabalho

1. Inventariar encoders/decoders reais e modos hardware em Windows/Mac/Android. Não confundir suporte de decoder com encoder no PC.
2. Comparar H.264 otimizado com candidatos na mesma resolução/FPS/cena; incluir texto colorido pequeno, gradientes, scroll, vídeo e cursor.
3. Medir bitrate, qualidade visual, tempo de encode/decode, latência p95, temperatura e bateria. Métrica objetiva sozinha não aprova legibilidade.
4. Projetar capabilities por codec/perfil/level, CONFIG, parameter sets e pedido de keyframe. Não reutilizar parser Annex-B AVC sem validar o formato novo.
5. Definir limites de buffers e fallback AVC se configure/encoder falhar; não alternar codec em loop.
6. Investigar matriz/range de cor, 8/10 bits, SDR/HDR e chroma. Não anunciar HDR ou 4:4:4 se a cadeia inteira não suportar.
7. Levantar implicações de distribuição/licenciamento em fontes apropriadas para decisão de produto; não declarar ausência de obrigações sem verificação.
8. Estimar custo de tamanho de app/dependências e hardware mínimo.

## Aceite

- [ ] Comparação reproduzível por hardware/codec, incluindo casos sem suporte.
- [ ] Contrato de negociação e fallback documentado.
- [ ] Evidência visual e de latência sustenta a recomendação.
- [ ] Decisão seleciona no máximo um próximo candidato para produção, ou mantém AVC.

## Testes do protótipo

Cobrir decoder ausente, hardware presente mas configuração recusada, mudança de geração, AU acima do limite e recuperação de keyframe após perda. Testar encoder candidato falhando em runtime e retorno a AVC sem loop de renegociação. Identificar quais combinações foram executadas fisicamente e quais existem apenas em fixtures.

Sem novo codec padrão, download de binários opacos ou perda de compatibilidade nesta tarefa. Protótipo fica atrás de opção experimental; rollback remove o experimento sem alterar o formato padrão.
