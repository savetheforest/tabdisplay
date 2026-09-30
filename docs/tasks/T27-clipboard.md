# T27 — Compartilhar texto pela área de transferência

Prioridade: P3. Tipo: Opcional. Status: Em validação.
Dependências: T02, T06, T09, T11.

## Objetivo

MVP de envio manual de texto entre PC e tablet autenticados. Sincronização contínua, imagens e arquivos ficam fora da primeira entrega.

## Implementação

1. Definir capability e mensagem com limite de bytes, encoding UTF-8 e direção. Rejeitar tamanho excessivo antes de alocar.
2. Oferecer ações explícitas “Enviar texto copiado” e “Copiar texto recebido”, com identificação da sessão.
3. Respeitar restrições Android de clipboard em background/foco; não recorrer a serviço de acessibilidade nem permissão invasiva para contornar a plataforma.
4. Manter recurso desligado inicialmente e consentimento por dispositivo; parear para vídeo não autoriza leitura contínua de clipboard.
5. Não executar, abrir URL, colar em aplicativo ou interpretar comandos automaticamente ao receber.
6. Não persistir histórico nem enviar texto para telemetria. Se a API marcar conteúdo sensível, respeitar a marcação e comportamento de prévia.
7. Se futuramente houver sync, usar IDs de origem/deduplicação para impedir ping-pong; documentar esse contrato sem implementá-lo no MVP.
8. Integrar revogação e desconexão: cancelar operações pendentes e descartar payloads associados à sessão anterior. O inbox transitório do PC agora é limpo quando a sessão que originou o payload termina.

## Aceite e testes

- [x] Unicode, quebras de linha e texto vazio têm semântica definida: são texto UTF-8; vazio é permitido e continua sem paste automático.
- [x] Payload acima do limite é recusado sem crash ou uso crescente de memória.
- [x] Mensagem não altera clipboard nem cola silenciosamente sem a política escolhida.
- [ ] Recurso desativado ou device revogado não consegue transferir — falta evidência física de revogação.
- [x] Conteúdo-sentinela não aparece em logs/exportação; o código não registra nem exporta o payload.

Testar ambos os sentidos com duas sessões para não enviar ao tablet errado, background Android e cancelamento. Usar apenas texto fictício.

Rollback remove capability e limpa buffers transitórios; não apagar o clipboard atual do usuário como efeito de desativação.
