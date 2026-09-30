# Relatório T29 — Codecs alternativos

Data: 29/09/2026  
Status: Concluída como spike

Recomendação: manter H.264/AVC como codec padrão e não adicionar HEVC/AV1 nesta versão. O contrato v3, keyframe, parameter sets, MediaCodec e encoders Windows/macOS estão calibrados para AVC; não há nesta sessão encoder/decoder físico comparável, medição de latência p95, bateria/temperatura, texto fino/gradiente ou análise de distribuição/licenciamento suficiente para aprovar um candidato.

Qualquer experimento futuro ficará atrás de capability explícita, configuração/level próprios, buffers bounded, fallback AVC único e geração/configuração coerentes. Não haverá alternância de codec em loop nem download de binário opaco.
