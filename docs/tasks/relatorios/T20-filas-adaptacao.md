# Relatório T20 — Filas e adaptação

Data: 29/09/2026  
Status: Em validação

## Entrega implementada

- O orçamento inicial por sessão está explícito: no máximo dois slots de frame bruto (o loop serial materializa apenas um), um quadro codificado em voo limitado a `MAX_VIDEO` e idade de frame de 250 ms.
- A captura/encode/escrita continuam seriais nesta etapa. Isso mantém a fila bruta em zero ou um item e descarta slots de tempo vencidos por `next_deadline_after`, sem acumular trabalho antigo para reproduzir depois.
- Um NAL maior que o limite de vídeo é rejeitado antes da escrita; não existe drop genérico de P-frames já codificados.
- A fila de áudio permanece bounded em 160 ms e a escrita consome no máximo dois pacotes por turno, com limite de bytes, preservando oportunidade para vídeo e mensagens de controle.
- A adaptação Auto agora distingue `rede`, `encoder`, `captura`, `tablet` e `tela` (ociosa). Cena estática não provoca downgrade, e contadores de perdas do tablet são tratados por delta para não manter um evento antigo como falha permanente.
- Histerese e cooldown existentes foram preservados: três amostras ruins para descer, 20 amostras calmas para subir e retenção de 60 s após downgrade. Perfis manuais não chamam a política Auto.
- O motivo efetivo é mostrado no status da sessão quando um rebuild de qualidade é solicitado. Não foi inventada uma classificação de temperatura: o protocolo atual não fornece telemetria térmica.

## Arquivos principais

- `desktop/src-tauri/src/server.rs`

## Validação local

Comando: `cargo test --locked` em `desktop/src-tauri`.

- 45 testes unitários passaram.
- 1 teste de áudio físico permaneceu ignorado.
- 1 teste de integração passou.
- Nenhuma falha.
- Testes novos cobrem cena estática sem adaptação, classificação por encoder/tablet, limites de memória declarados, recuperação com histerese e fairness de áudio por quantidade/bytes no fio, incluindo o overhead de AUDIO v1.

## Validação ainda pendente

Não foi feito shaping de rede global nem benchmark físico. Ainda faltam harnesses isolados de rede lenta/perda/jitter, encoder lento, decoder lento, movimento após cena estática, fairness contínuo sob sessão longa e medição de convergência em até 2 s no hardware de referência.

O caminho serial foi mantido porque não há benchmark que justifique introduzir produtor/encoder/escritor concorrentes; qualquer separação futura deve manter fila recente limitada, geração/configuração e pedido de IDR coerentes.
