# Relatório T34 — Serviço privilegiado e limites

Data: 29/09/2026  
Status: Em validação

## Entrega implementada

- O pipe do serviço documenta a política de acesso: SYSTEM/admin têm controle total; usuários interativos locais têm somente o canal necessário ao app. A superfície não é publicada em TCP e não aceita controle remoto.
- O serviço limita clientes ativos a 8 e comandos a 20 por segundo. Linhas são lidas com teto de 256 bytes, rejeitando UTF-8 inválido, newline ausente ou comando longo antes de processar o driver.
- `MODE`/`ENABLE` validam dimensões múltiplas de 16, faixa 16–7680, máximo de 16 Mpixels e ausência de argumentos extras antes de tocar XML/driver.
- Operações de topologia são serializadas num mutex próprio, sem manter o contador de leases durante I/O de driver. Release também passa pela mesma serialização e continua idempotente por conexão.
- O servidor TCP limita handshakes concorrentes a 16 antes do TLS/HELLO; isso evita que `MAX_TABLETS=2` seja confundido com limite de recursos de conexão.
- CSP do Tauri deixou de ser `null`: só permite assets locais, IPC Tauri e estilos inline necessários ao HTML existente. A capability continua somente `core:default`; o frontend usa `textContent` para dados externos, mantendo `innerHTML` apenas para templates estáticos locais.
- O guia de primeira execução separa templates locais de diagnósticos: `DRIVER[s.driver]` não é mais interpolado em HTML e é aplicado no nó `.muted` via `textContent`; nomes/erros já seguiam a mesma regra.
- O leitor do pipe usa `PIPE_NOWAIT`, dorme em pequenos intervalos quando não há dados e encerra com timeout de 5 s sem progresso; bytes já recebidos reiniciam o prazo, preservando o limite de 256 bytes e evitando thread presa por peer lento.
- O orçamento XML de modos, backup/persistência e marcador de ownership do driver foram tratados nos T18/T19/T24 e permanecem ativos.

## Arquivos principais

- `desktop/src-tauri/src/win/service.rs`
- `desktop/src-tauri/src/server.rs`
- `desktop/src-tauri/tauri.conf.json`
- `desktop/src-tauri/windows/hooks.nsh`

## Validação local

O código Windows passou pela suíte `cargo test --locked` nesta máquina: 72 testes unitários passaram, 1 teste físico de áudio ficou ignorado e 1 teste de integração passou; `node --check desktop/src/main.js` também passou. Os testes unitários `win::service::tests::command_reader_bounds_long_and_partial_input`, `win::service::tests::invalid_dimensions_and_extra_arguments_stop_before_driver` e `win::service::tests::slow_peer_times_out_without_growing_or_holding_the_command` confirmam o limite de 256 bytes, entrada parcial/CRLF, timeout de progresso e rejeição de dimensões/argumentos extras antes do driver. Os testes reais de pipe/ACL, interrupção do serviço e reparse points exigem VM/Windows designado e não foram executados no desktop em uso.

Após a correção do guia, o release foi recompilado e relançado como PID 10864.
Com Wi‑Fi temporariamente desligado, a sessão USB confirmou
`127.0.0.1:7070 ↔ 127.0.0.1:54743`, com `MainActivity`/`SurfaceView` ativos;
o Wi‑Fi foi restaurado e nenhum dado foi apagado. Isso valida a regressão da
sessão, não as ACLs reais do serviço.

Após integrar o timeout de peer lento, o release final foi recompilado e
relançado como PID 2884. A sessão USB confirmou
`127.0.0.1:7070 ↔ 127.0.0.1:54377`, com `MainActivity`/`SurfaceView` ativos;
o Wi‑Fi foi restaurado e nenhum dado foi apagado.

## Limitações registradas

A ACL `IU` restringe o pipe a usuários interativos locais, mas não implementa ainda vínculo exclusivo ao SID do usuário que instalou o TabDisplay; isso fica explícito como hardening futuro. Também não foi introduzido cancelamento overlapped de uma leitura byte-a-byte; o teto de 256 bytes e limite de clientes reduzem abuso, mas uma VM deve testar o caso de peer sem progresso antes de declarar o aceite completo.
