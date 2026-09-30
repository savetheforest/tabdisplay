# T23 — Fazer CI verificar comportamento e compatibilidade reais

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T01.

## Contexto

CI compila Windows/Mac, testa Rust no Windows e executa task Android atualmente sem fontes de teste. Build de main gera release Android sem a mesma etapa explícita de testes do PR. Formatação não passa no estado auditado; Mac só compila por limitação de runtime descrita no workflow.

## Implementação

1. Consolidar checks de PR/main: testes Rust aplicáveis, testes JVM Android reais, assemble e lint pertinente. O job de PR também executa `cargo fmt --all -- --check` e `node --check ../src/main.js` em ambos os runners desktop.
2. Separar testes puros, instrumentados, integração loopback e hardware. Não incluir captura/injeção de entrada real nos runners sem ambiente preparado.
3. Adicionar fixtures cruzadas de T02 e matriz de compatibilidade antigo/novo, com falhas não ignoradas.
4. Tratar formatting baseline em mudança mecânica separada ou configurar padrão acordado; não reformatar o projeto inteiro junto de correção de FPS.
5. Introduzir clippy/lint gradualmente com baseline explícito de warnings, sem desabilitar categorias inteiras para obter verde.
6. Diagnosticar a limitação dos testes Mac e habilitar os que rodam; não afirmar validação de runtime apenas por cargo check.
7. Adicionar emulador de API mínima e API recente para lifecycle/Compose. Benchmarks de emulador não servem como performance de decoder físico.
8. Criar harness com atraso/desconexão/EOF/parcial framing. Usar portas efêmeras e cleanup para não deixar processos no runner.
9. Armazenar relatórios úteis sem tokens, logs pessoais ou mídia da tela; definir retenção limitada.
10. Versionar a matriz de VALIDACAO.md por release, com dispositivos realmente testados e pendências.
11. Evitar execução de código de PR não confiável com secrets de assinatura/publicação; checks de PR não precisam dessas credenciais.

## Critérios de aceite

- [ ] CI falha quando teste falha; Android não passa apenas com NO-SOURCE.
- [ ] Main e PR têm checks coerentes antes de gerar artefatos distribuíveis.
- [ ] Incompatibilidade de protocolo/fixtures é detectada automaticamente.
- [ ] Testes por plataforma estão nomeados sem ocultar skips relevantes.
- [ ] Suíte tem runtime razoável e falha determinística, sem sleeps frágeis.

## Validação e entrega

Executar os comandos locais disponíveis; inspecionar workflow/diff. Disparar CI remoto apenas se autorizado no trabalho vigente. Registrar os jobs que ainda dependem de runner/segredos/hardware.

Rollback preserva checks anteriores; não remover teste de assinatura para contornar falha. Não incorporar automaticamente atualizações massivas de dependências nesta tarefa.
