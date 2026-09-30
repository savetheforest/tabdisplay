# T10 — Pausar vídeo e ajustar consumo sem perder a sessão

Prioridade: P2. Tipo: Core. Status: Em validação.
Dependências: T02, T05, T09. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Contexto

detach libera o decoder, mas o servidor continua capturando/codificando/enviando. FLAG_KEEP_SCREEN_ON é aplicado desde a tela de conexão. Há oportunidade de poupar bateria e CPU sem desmontar o monitor virtual.

## Implementação

1. Separar visibilidade da Surface, tela apagada, Activity em background, sessão conectada e preferência do usuário.
2. Implementar pausa/resume negociados por geração. Definir ACK, idempotência, heartbeat e prazo de retenção da sessão.
3. Pausado: interromper produção de vídeo e drenar/descartar estado conforme contrato, mantendo monitor e controle de sessão. Não remover monitor e mover janelas do usuário.
4. Resume: fornecer CONFIG quando necessário e IDR atual; nunca reproduzir backlog da pausa. Não usar RESIZE para toda retomada se a dimensão não mudou.
5. Permitir política de áudio independente: parar com vídeo ou continuar explicitamente. Silenciar localmente não deve alterar volume do PC.
6. Manter tela ligada só durante uso quando configurado. Desativar wake/multicast locks na tela de conexão oculta e fora da necessidade.
7. Adicionar perfil economia: teto de FPS/bitrate e comportamento com bateria baixa. Respeitar overrides do usuário e a negociação T03.
8. Usar estado térmico oficial quando disponível e não inventar temperatura em graus se a API não fornecer. Mostrar redução por temperatura separada de rede lenta.
9. Aplicar histerese e cooldown para evitar oscilar; pausar override temporário antes de persistir uma configuração degradada.
10. Medir tempo de CPU/GPU e tráfego antes/depois com T05; não afirmar economia por simples redução de um contador.

## Critérios de aceite

- [ ] Vídeo pausado não continua produzindo frames de forma sustentada.
- [ ] Retomar não move janelas nem espera uma fila velha.
- [ ] Heartbeat mantém conectividade e desconexão ainda encerra recursos.
- [ ] Áudio continua ou para conforme preferência documentada.
- [ ] Indicadores distinguem economia/térmico/rede e respeitam modo manual.

## Testes e limites

Pausa durante CONFIG, resume duplicado, pause→close, cabo removido pausado, duas sessões com só uma pausada e lock/unlock repetido. Medir 10 minutos ativo e pausado, no mesmo aparelho e cena. Descrever limitações de fabricante/Doze sem prometer execução infinita.

Rollback: feature flag de pausa negociada; cliente legado mantém comportamento atual. Não manter wake lock infinito como solução para incompatibilidade.
