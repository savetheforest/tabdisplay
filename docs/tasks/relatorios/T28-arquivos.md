# Relatório T28 — Arquivos

Data: 29/09/2026  
Status: Em validação

Foi adicionada uma base segura, ainda não exposta ao protocolo: `desktop/src-tauri/src/files.rs` valida ID de transferência, nomes Unicode com limite de 255 bytes, traversal/separadores, caracteres inválidos e nomes reservados do Windows; limita o arquivo a 4 GiB; recebe chunks sequenciais de até 64 KiB; calcula SHA-256 incrementalmente; rejeita excesso, lacunas, hash incorreto e dados após cancelamento. O estado guarda somente contadores e o estado do hash, não o arquivo inteiro.

O desktop agora tem também um receptor local bounded: valida o diretório
selecionado, recusa symlink/reparse point no diretório/destino, cria um `.part`
exclusivo, grava os chunks diretamente sem acumular o arquivo, verifica
hash/tamanho e faz commit sem substituir um destino que apareceu durante a
transferência. Cancelamento, hash inválido e Drop limpam somente o temporário
pertencente à transferência.

Foi adicionado `desktop/src-tauri/src/file_protocol.rs` como contrato wire
versionado e bounded para a próxima integração: BEGIN/END/CANCEL são JSON
limitado a 64 KiB e CHUNK tem cabeçalho binário fixo, ID de transferência,
offset e payload limitado a 64 KiB. O módulo valida versão, ID, hash, nome,
motivo de cancelamento, tamanho e truncamento; ele ainda não anuncia capability,
não abre porta/canal paralelo e não aceita mensagens fora de uma sessão já
autenticada.

Testes unitários cobrem arquivo vazio, ID/tamanho/nome inválidos, nomes maliciosos, Unicode, hash correto/incorreto, chunks fora de ordem, chunks grandes, excesso do tamanho anunciado e cancelamento; os novos testes cobrem commit verificado, cleanup, corrida de destino existente e diretório inválido. A suíte completa será registrada no checkpoint global após o rebuild.

Após o rebuild `cargo build --locked --release`, o executável do PC foi reiniciado. O tablet físico permaneceu conectado por USB com `adb reverse tcp:7070 tcp:7070`, sessão TCP ativa, `SurfaceView` 1152x640 e UI reportando `52 fps · 8 ms · 8,7 Mbps`; esta evidência valida a regressão da sessão, não uma transferência de arquivo ainda inexistente.

Após integrar o receptor local e o contrato wire, a suíte desktop passou com 80
testes unitários, 1 ignorado por áudio físico e 1 integração. O release final foi recompilado e
relançado como PID 18756; com Wi‑Fi temporariamente desligado, a seleção
“Conexão local pelo ADB” confirmou `127.0.0.1:7070 ↔ 127.0.0.1:58490`, com
`MainActivity`/`SurfaceView` ativos. O Wi‑Fi foi restaurado sem apagar dados;
como não há capability/UI/canal de arquivo, nenhuma transferência física foi
contabilizada.

O futuro MVP ainda deve usar transferência autenticada na sessão, aceite no receptor, Storage Access Framework/seletor do SO, capability/mensagem de protocolo efetivamente roteada e backpressure com janela limitada. O núcleo local e o contrato wire já cobrem temporário exclusivo, hash antes da finalização, cleanup próprio, preservação de destino e limites de framing; integração, disco cheio observado, symlink/junction em toda a árvore, revogação e orçamento separado da mídia continuam pendentes. Não executará/abrirá arquivos nem retomará offsets sem revalidação.

Bloqueios reais: falta decisão de destino/UX e matriz de permissões Android/Windows/macOS; não foi criado canal experimental, capability ou mensagem `FILE`, portanto não há ainda escrita na pasta do usuário pelo produto nem validação física de transferência.
