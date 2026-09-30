# T22 — Tornar conexão e controles mobile claros e acessíveis

Prioridade: P2. Tipo: Core. Status: Em validação.
Dependências: T03, T05, T09, T12, T13, T18.

## Objetivo

O usuário deve entender como conectar, qual caminho está ativo e por que a imagem está lenta. MainActivity concentra conexão, pareamento e sessão; ampliar organização apenas onde necessário para manter estados testáveis.

## Implementação

1. Desenhar estados de busca, sem PC, conectando, autorização USB pendente, pareando, ativo, pausado, reconectando e erro. Cada erro tem ação pertinente e opção de cancelar.
2. Oferecer entrada USB guiada usando estado real: cabo só de carga, ADB ausente/unauthorized e rede USB. Não criar um botão que prometa ligar tethering programaticamente.
3. Exibir nome/identidade e transporte conhecido, com “Rede local” para caso indeterminado. Explicar operação sem internet e instalação do desktop.
4. Criar menu/folha de controles acessível: qualidade, FPS disponível, áudio, manter tela ligada, orientação, diagnóstico e desconectar. Mostrar configuração aplicada, não apenas pedido otimista.
5. Facilitar acesso sem cobrir vídeo: área de toque adequada, contraste, TalkBack, foco, escala de fonte e insets/cutout.
6. Implementar rotação bloqueada/opcional e comportamento em celular, tablet, retrato, paisagem, split-screen e tela grande. Layout respeita tamanho atual da janela, não só tamanho físico do aparelho.
7. Se houver zoom/pan local para telas pequenas, separar gesto local de gesto enviado ao PC, com modo explícito e transformação de input testada.
8. Reconexão preserva contexto e informa progresso sem prompt piscando; cancelamento impede autoconnect até nova intenção.
9. Adicionar gerenciamento de PCs conhecidos/esquecer no mobile ligado à política T11; distinguir esquecer local de revogar no servidor.
10. Mostrar estatísticas opt-in com significado correto e ação “exportar diagnóstico” local sanitizada.
11. Usar strings de recurso; PT-BR consistente. Internacionalização prepara estrutura, sem inventar tradução automática de mensagens do sistema.

## Critérios de aceite

- [ ] Usuário consegue completar conexão USB e Wi-Fi seguindo apenas a UI.
- [ ] Estado/error não depende só de cor e é anunciado para acessibilidade.
- [ ] Menu tem alvo de toque adequado, não só uma aba visual de 20 dp.
- [ ] Controles não oferecem combinações impossíveis de vídeo.
- [ ] Mudança de janela/fonte não esconde desconectar/reconectar.
- [ ] Android 11 funciona sem invocar API nova sem guard.

## Validação

Testes de estado Compose e fluxo instrumentado; revisão visual real em aparelho pequeno, tablet e fonte ampliada; navegação TalkBack/teclado. Conteúdo remoto da tela não vira automaticamente acessível: documentar a limitação, sem anunciar suporte semântico completo ao desktop.

Rollback por componente de UI mantendo controlador T09. Teclado/macros/clipboard são tarefas opcionais separadas.
