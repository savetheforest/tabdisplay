# Relatório T23 — CI e qualidade

Data: 29/09/2026  
Status: Em validação

## Entrega implementada

- PR Android agora executa `assembleDebug`, `testDebugUnitTest` e `lintDebug`; não depende apenas de uma task sem testes.
- Build de main executa `testDebugUnitTest` e `lintRelease` antes de `assembleRelease`/`bundleRelease`.
- A matriz desktop continua separando Windows (testes Rust executáveis) de macOS (compilação/check no runner). O comentário do workflow nomeia a limitação de `libswift_Concurrency`; não há skip silencioso.
- Fixtures compartilhadas de protocolo/entrada já são consumidas por testes Rust e JVM desde T01/T02.
- A matriz versionada de `docs/tasks/VALIDACAO.md` ganhou registro explícito da release v0.2.0 e da execução de 29/09/2026, distinguindo host, hardware real, runtime e compilação.
- Não foi introduzida atualização massiva de dependências, segredo de assinatura em PR ou execução de código de PR com credenciais.
- O job desktop de PR agora instala Node 22, verifica `cargo fmt --all -- --check` e a sintaxe de `desktop/src/main.js` em Windows e macOS antes do teste Rust condicionado ao runner Windows.

## Arquivos

- `.github/workflows/ci.yml`
- `.github/workflows/build.yml`
- `docs/tasks/VALIDACAO.md`

## Validação local

- Desktop: `cargo test --locked` passou com 76 unitários, 1 ignorado por áudio físico e 1 integração; `node --check desktop/src/main.js` e `cargo fmt --all -- --check` passaram localmente.
- Android: `testDebugUnitTest assembleDebug` passou com 44 tarefas.
- `lintDebug` local permanece bloqueado pelo `android/local.properties` gerado com caminho Windows escapado; em CI o SDK/runner é responsável por esse arquivo.
- O baseline de formatação agora passa localmente; o workflow falhará se uma alteração futura introduzir diferenças, sem habilitar clippy com warnings não resolvidos.
- CI remoto não foi disparado.

## Pendências

Ainda faltam observar um run real de CI, habilitar testes macOS quando a limitação do runner for resolvida, emuladores API mínima/recente, harnesses longos de EOF/atraso e revisão de baseline de formatting/clippy. Esses checks não foram mascarados por desabilitação global.
