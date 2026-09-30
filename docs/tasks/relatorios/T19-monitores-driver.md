# Relatório T19 — Monitores, driver e recuperação

Data: 29/09/2026  
Status: Em validação

## Entrega implementada

- A captura Windows agora resolve `None` para o monitor marcado como primário pelo GDI; a ordem da enumeração DXGI não é mais usada como política de seleção.
- `open_capture` mantém o retry limitado para uma saída que acabou de aparecer, mas retorna erro explícito quando a saída escolhida desaparece ou falha. Não há fallback silencioso para o primário.
- Falha de attach/configuração do monitor virtual encerra a sessão com erro informado. A sessão não muda automaticamente para Mirror.
- `VirtualDisplay` valida resolução, posição e refresh efetivos depois de `ChangeDisplaySettingsExW`. Se o refresh pedido for recusado, tenta @60 e valida também o retorno da tentativa de fallback.
- O status da sessão inclui o tamanho e refresh efetivos do monitor virtual quando disponíveis.
- O detach do monitor ocorre antes de liberar o nome no conjunto de leases; falha de detach gera telemetria. Isso reduz a corrida entre desclaim e attach de outra sessão.
- A lista XML do driver é limitada a 24 entradas de resolução, remove refresh global acima de 120 Hz, ignora 30 Hz e continua idempotente quando nada mudou.
- Não foram usados comandos de reload do driver. A configuração continua passando pelo lease do serviço e as alterações de modo são reaplicadas somente quando o modo/posição efetivos realmente divergem.

## Arquivos principais

- `desktop/src-tauri/src/win/capture.rs`
- `desktop/src-tauri/src/win/display.rs`
- `desktop/src-tauri/src/server.rs`

## Validação local

Comando: `cargo test --locked` em `desktop/src-tauri`.

- 45 testes unitários passaram.
- 1 teste de áudio físico permaneceu ignorado (`loopback_captures_what_plays`).
- 1 teste de integração passou (`update_signature`).
- Nenhuma falha.
- Permanecem apenas avisos conhecidos de `find_device` não usado, mensagens do linker e canonicalização de `C:\Users\jhona`.

Testes específicos adicionados: uma configuração XML com mais que o orçamento é reduzida a no máximo 24 modos e uma segunda aplicação idêntica não altera o arquivo; a contagem de monitores ausente ou não terminada permanece inalterada; a limpeza remove somente modos/refreshes de estoque e preserva o restante do XML.

## Validação ainda pendente

Não foi feita mutação no tablet USB detectado. A instalação/abertura do APK atualizado foi rejeitada pela aprovação de segurança para a mutação do dispositivo; portanto não há alegação de validação física.

Continuam pendentes os testes designados com dois tablets, rotação, lock/UAC, sleep/wake, hotplug de monitor, nomes GDI trocados, modo recusado, falha de detach e reinício controlado do serviço. Também não há host macOS nesta sessão para validar a separação das APIs de display/captura.

Não houve alteração real no XML do driver, reload, desinstalação ou restauração de driver de terceiros.
