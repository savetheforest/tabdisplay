# T01 — Construir uma base de testes útil para os dois clientes

Prioridade: P1. Tipo: Core. Status: Concluída.
Dependências: nenhuma. Aplicam-se GUIA-EXECUCAO.md e VALIDACAO.md.

## Problema e objetivo

Rust tem testes pontuais; Android compila, mas testDebugUnitTest não executa testes. Criar apoio que permita reproduzir framing, erros de sessão e decisões de capacidade sem um tablet conectado. Não reescrever a aplicação inteira para torná-la testável.

## Onde investigar

- android/app/build.gradle.kts e android/app/src/main/java/com/tabdisplay/{Stream,Input,Discovery,MainActivity}.kt.
- desktop/src-tauri/src/{server,input,pairing,tls,settings}.rs.
- desktop/src-tauri/tests/update_signature.rs e .github/workflows/ci.yml.

## Implementação

1. Registrar o baseline de build e os testes existentes. Preservar o teste de assinatura e o de áudio explicitamente ignorado.
2. Extrair somente lógica pura necessária: framing, parsing, seleção de capacidades e decisões de reconexão. APIs Android continuam atrás de adaptadores pequenos.
3. Criar fixtures canônicas compartilhadas por Rust e Kotlin para HELLO/CONFIG v3, INPUT e SCROLL, com bytes big-endian conhecidos e resultados esperados.
4. Acrescentar casos inválidos: header truncado, tamanho maior que o limite, JSON malformado, float não finito, enum desconhecido e EOF no meio do payload.
5. Introduzir relógio monotônico e transporte falsos onde timeouts/retries precisarem ser determinísticos. Não usar sleeps longos em testes.
6. Isolar Settings/pareamento em diretório temporário e evitar disputa entre testes sobre singletons globais.
7. Criar smoke de conexão com servidor/cliente de teste em loopback e porta efêmera; sem iniciar Tauri, capturar a tela, mexer no driver ou conectar a dispositivos do usuário.
8. Documentar dependências de testes Kotlin/JVM versus instrumentados; adicionar apenas as necessárias.

## Cuidados e fora de escopo

- Não salvar tokens/certificados reais nas fixtures. Chaves de teste devem ser geradas ou explicitamente identificadas e nunca usadas em release.
- Não criar teste que apenas repita a função sem verificar contrato externo.
- Esta tarefa não corrige todos os comportamentos: testes de regressão específicos acompanham cada tarefa.
- Não renomear em massa classes/arquivos, nem substituir Compose, Rust ou Tauri.

## Critérios de aceite

- [ ] Android executa pelo menos um teste real de framing e um de entrada, com resultados verificáveis.
- [ ] Rust e Kotlin aceitam/rejeitam as mesmas fixtures de contrato.
- [ ] Testes não dependem de segredos, conta externa, hardware, porta 7070 livre ou estado persistido do app.
- [ ] Uma falha de framing introduzida localmente seria detectada; não manter a mutação no código entregue.
- [ ] Comandos e distinção entre teste JVM/instrumentado estão documentados.

## Validação e entrega

Executar cargo test --locked no Windows e gradlew.bat testDebugUnitTest assembleDebug. Confirmar ausência de NO-SOURCE para os novos testes. Se houver ambiente Mac, executar as partes suportadas sem esconder a limitação de runtime já anotada no CI.

Rollback: remover somente a extração/adaptadores introduzidos, mantendo fixtures e evidências úteis; nenhuma migração de dados é necessária.
