# T26 — Teclado, trackpad e atalhos para controlar o PC

Prioridade: P3. Tipo: Opcional. Status: Em validação.
Dependências: T02, T08, T09, T11, T22.

## Objetivo e escopo inicial

Permitir digitar pelo tablet, usar mouse/trackpad externo e uma barra de atalhos. Input atual transmite toque/caneta e scroll; não existe contrato de teclado completo.

## Implementação

1. Definir mensagens separadas para tecla física, modificadores, texto Unicode e mouse relativo/absoluto. Não confundir keycode Android com scancode Windows ou keycode Mac.
2. Modelar composição IME: acentos, dead keys, seleção, delete, enter e caracteres fora do BMP. Evitar duplicar texto ao receber tecla física e commit do IME.
3. Criar adaptadores Windows/Mac que respeitem layout; texto direto e atalho precisam caminhos distintos.
4. Rastrear teclas/botões pressionados por sessão e soltar ao perder foco, desconectar ou desabilitar controle.
5. Integrar mouse secundário, hover e scroll de precisão. Modo trackpad local não deve injetar também o mesmo gesto como toque nativo.
6. Criar atalhos configuráveis como sequências limitadas de teclas com rótulo e prévia. Proibir execução arbitrária de shell/scripts e macros recebidas de peer sem escolha local.
7. Respeitar a opção “Controlar o PC” e permissões do SO. Não tentar contornar secure desktop/UAC.
8. Informar que a entrada vai para a janela com foco no PC; não assumir que é sempre uma aplicação no monitor do tablet.
9. Usar filas limitadas que preservem key up/down e cancelamento; condensar apenas movimentos quando seguro.

## Aceite e testes

- [ ] Português/acentos, emoji, teclas especiais e modificadores produzem resultado correto.
- [ ] Nenhuma tecla fica pressionada após queda da conexão.
- [ ] Atalho só ocorre por ação do usuário e respeita controle desativado.
- [ ] Teclado externo e virtual não duplicam eventos.
- [ ] Trackpad tem modo explícito e input mapeado corretamente.

Testar em editor de texto de teste, com teclado ABNT2 e layout alternativo; instrumentar fakes antes de injetar em aplicativo real. Não usar campos de senha reais como fixture.

Rollback desativa capability e remove controles, mantendo compatibilidade com protocolo antigo. Macros não viram mecanismo genérico de automação remota.
