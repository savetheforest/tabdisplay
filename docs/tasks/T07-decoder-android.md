# T07 — Tornar o decoder Android recuperável e consistente com a Surface

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T02, T03, T06. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Contexto

Stream.kt mistura leitura de mensagens e espera por input buffer de até 500 ms. Índices ficam em uma fila compartilhada entre instâncias de MediaCodec. CONFIG, detach, attach e callbacks podem se sobrepor; o impacto exato dessas corridas precisa reprodução.

## Implementação

1. Modelar gerações do decoder: sem Surface, configurando, esperando IDR, executando e encerrado.
2. Cada callback e buffer deve carregar identidade/geração do codec. Rejeitar callback antigo; limpar fila sozinha não é prova de que não chegará um callback atrasado.
3. Serializar configure/start/stop/release/queue no executor apropriado, com ownership claro da Surface; evitar espera de callback segurando lock que ele precisa.
4. Separar leitura de controle da espera do decoder usando filas limitadas e a política de mídia de T02. A fila inicial deve favorecer segurança de decode, não descarte arbitrário.
5. Validar frame.size contra capacidade do input buffer. Definir reação para AU maior que o buffer: reconfiguração suportada ou erro/IDR, nunca put cego.
6. Ao perder referência, flushar/recriar conforme API, descartar até IDR válido e solicitar keyframe ao PC com rate limit. Preservar SPS/PPS.
7. Integrar pedido de IDR em encoders Windows, Mac e CPU, ou fallback explícito de recriação do encoder quando a API não permitir. Não presumir GOP periódico no fallback CPU.
8. Ao attach sem mudança de tamanho, restaurar vídeo sem reiniciar o monitor virtual. CONFIG com geração e IDR devem preceder o primeiro frame utilizável.
9. Tratar CodecException com causa/diagnóstico e fallback limitado. Garantir release mesmo se stop lançar exceção; revisar áudio no mesmo padrão.
10. Manter close idempotente e impedir criação de codec/Audio depois de fechado.

## Critérios de aceite

- [ ] CONFIG seguido de novo CONFIG não injeta índice do codec antigo no novo.
- [ ] Lock/unlock, rotação e troca rápida de app não deixam tela preta permanente.
- [ ] Decoder lento não impede resposta ao heartbeat indefinidamente.
- [ ] Recuperação começa em keyframe válido; não há P-frames órfãos.
- [ ] Cada codec/Surface é liberado uma vez, inclusive em falha parcial.

## Testes

Fakes para callbacks atrasados, index repetido, buffer pequeno e falha de configure/stop. Instrumentado: 30 ciclos de resize/lock/attach, rotação durante CONFIG e duas mudanças rápidas de perfil. Medir memória/threads e tempo até primeira imagem.

Meta inicial de recuperação: até dois segundos em aparelho de referência com link estável após pedido de IDR; tratar como meta proposta a validar, não SLA universal.

Rollback preserva geração/limites e pode restaurar decoder anterior por flag interna; não desabilitar a checagem de capacidade para esconder falhas.
