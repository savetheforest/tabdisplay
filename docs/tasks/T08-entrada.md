# T08 — Garantir entrada correta e liberar contatos ao encerrar

Prioridade: P1. Tipo: Core. Status: Em validação.
Dependências: T02, T07. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Contexto e escopo

Input.kt envia contatos normalizados; Windows injeta touch/pen ou mouse; Mac emula gestos por mouse/atalhos. Mouse e pen podem precisar UP explícito quando a sessão cai. O código atual não deve ser anunciado como caneta nativa equivalente em todos os sistemas.

## Arquivos

Input.kt; MainActivity.kt: listeners; Stream.kt: sender; input.rs; win/input.rs; mac/input.rs; server.rs: target.

## Implementação

1. Especificar contrato de DOWN/MOVE/UP/CANCEL/HOVER/LEAVE e mapear ACTION_CANCEL explicitamente, sem gerar um clique inesperado.
2. Rastrear contatos e botões realmente injetados por sessão; ao desconectar, pausar input, trocar modo/monitor ou perder foco, liberar somente os eventos sintéticos pertencentes à sessão.
3. No modo mouse, manter o pointerId que iniciou o gesto. Não trocar implicitamente para o “primeiro elemento” quando outro dedo sai.
4. Validar quantidade máxima, IDs, coordenadas, pressão, inclinação e float finito no parser. Respeitar o limite real do dispositivo sintético.
5. Limitar a fila de input. Condensar MOVE apenas entre eventos equivalentes do mesmo gesto/geração; preservar DOWN/UP/CANCEL, teclas futuras e ordem.
6. Unificar transformação viewport→vídeo→monitor, considerando barras pretas, escala, orientação, múltiplos monitores e coordenadas negativas.
7. Tratar mouse externo/trackpad como capacidade própria. Hoje hover não-pen é ignorado; não afirmar suporte completo até T26.
8. Revisar palm rejection durante caneta: oferecer política explícita e manter gestos disponíveis quando caneta ausente; não descartar todos os dedos sempre.
9. Conferir erros de InjectSyntheticPointerInput/SendInput e permissão Mac; status deve distinguir “vídeo conectado” de “controle indisponível”.
10. Documentar limitações Mac de pinch por atalhos e pressão via mouse; não usar APIs privadas de gesto sem decisão separada.

## Critérios de aceite

- [ ] Desconexão no meio de drag não deixa botão/contato sintético preso.
- [ ] Dois dedos levantados em ordens distintas não trocam o dono do mouse.
- [ ] Rotação/letterbox mantêm alvos em cantos/centro corretos.
- [ ] Falta de permissão desativa controle com mensagem, mantendo vídeo quando permitido.
- [ ] Fila congestionada preserva término dos gestos e tem limite comprovado.

## Testes e rollback

Fixtures Kotlin→Rust, sequências multi-touch e pen/eraser/hover, valores inválidos e CANCEL. Fakes para o injetor evitam controlar o desktop nos testes unitários. Testar manualmente em app de desenho e app que só aceita mouse, com cenários Windows/Mac separados.

Rollback por modo de input; sempre liberar eventos da sessão antes de trocar implementação.
