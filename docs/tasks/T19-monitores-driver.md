# T19 — Controlar monitores sem loops de rebuild nem fallback inesperado

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T04, T18, T34.

## Problemas e objetivo

O projeto já evita reaplicar modo idêntico, por correção recente de loops com dois tablets. Preservar isso. open_capture pode cair silenciosamente no principal após falha do monitor escolhido; find_output(None) escolhe o primeiro enumerado, sem checar explicitamente o principal. Capturar outra tela pode expor conteúdo diferente do solicitado.

## Arquivos

server.rs: open_capture/stream_once; win/display.rs; win/capture.rs; win/service.rs; win/driver.rs; mac/display.rs; options/status em lib.rs.

## Implementação

1. Identificar monitor principal explicitamente e distinguir identidade estável de nome GDI transitório. Detectar display que desaparece/retorna.
2. Tornar fallback de captura explícito: quando monitor escolhido/virtual falhar, pausar e informar; espelhar principal apenas se houver política previamente escolhida pelo usuário.
3. Modelar ownership do monitor/lease por sessão, incluindo falha parcial de attach. Um cliente não pode ocupar/reconfigurar o monitor de outro.
4. Coordenar mudanças de topologia global, pois um resize pode invalidar captura de outra sessão. Reabrir captura com backoff limitado sem reconfigurar o mesmo modo de novo.
5. Verificar resolução, refresh e posição efetivos, não só largura/altura. Exibir fallback real para 60 Hz quando 90/120 não for aplicado.
6. Verificar retorno de ambas as chamadas ChangeDisplaySettingsEx, inclusive apply global e tentativa de fallback.
7. Preservar layout/posição escolhidos pelo usuário quando possível e evitar deslocar toda a área de trabalho ao reconectar.
8. Controlar adição de modos XML com orçamento de resoluções×refresh. Não acumular infinitamente tamanhos criados por rotação/split-screen.
9. Antes de adicionar modo novo que exige reinício do driver, coordenar todas as sessões e indicar impacto. Nunca usar os comandos de reload documentados como instáveis.
10. Encerrar/restituir leases em erro/sleep/close e impedir corrida entre desclaim e detach.
11. No Mac, separar API privada de display das APIs públicas de captura e documentar suporte testado.

## Critérios de aceite

- [ ] Falha de monitor selecionado não transmite automaticamente outra tela.
- [ ] Dois tablets não compartilham monitor e não entram em rebuild contínuo.
- [ ] Refresh/posição exibidos correspondem ao efetivamente aplicado.
- [ ] Sessão encerrada libera somente seu monitor/lease.
- [ ] Modos XML ficam limitados e alterações idênticas são idempotentes.

## Testes e rollback

Fakes para enumeração fora de ordem, troca de nomes GDI, modo recusado e detach falhando. Testes físicos designados: dois tablets, rotação, lock/UAC, sleep/wake, hotplug de monitor e reinício controlado do serviço.

Guardar backup de configuração do driver antes de escrita real e preservar instalação de driver preexistente. Rollback restaura apenas alterações pertencentes ao TabDisplay; não desinstalar driver de terceiros.
