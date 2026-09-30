# T28 — Transferir arquivos localmente com destino escolhido

Prioridade: P3. Tipo: Opcional. Status: Em validação.
Dependências: T02, T06, T11, T20.

## Objetivo

Enviar um arquivo escolhido entre aparelhos pareados, com progresso, cancelamento e verificação de integridade, sem degradar input/vídeo. Não sincronizar pastas automaticamente.

## Implementação

Base segura adicionada em `desktop/src-tauri/src/files.rs`, sem expor capability,
mensagem de protocolo, UI ou ação de usuário: nomes são rejeitados quando têm
traversal, separadores, caracteres inválidos, controle, sufixo ambíguo ou nome
reservado do Windows; metadados validam ID não nulo, tamanho de até 4 GiB e
nome UTF-8 de até 255 bytes; o receptor processa chunks sequenciais de até 64
KiB e atualiza SHA-256 incrementalmente. O estado não guarda o arquivo inteiro
em RAM e permite cancelamento antes do fechamento.

O núcleo agora também possui um receptor local: exige diretório existente que
não seja symlink/reparse point, cria `.part` exclusivo, escreve somente chunks
limitados, verifica hash/tamanho e renomeia para o destino sem substituir um
arquivo que apareceu durante a transferência. O ciclo remove apenas o
temporário que criou; a seleção real pelo SO/SAF e o canal autenticado ainda
não estão integrados.

1. Especificar transferência com ID aleatório, nome de apresentação, tamanho conhecido/limite, chunks limitados e hash final. Integridade não substitui autenticação.
2. Solicitar aceite no receptor e destino via seletor do SO/Storage Access Framework. Definir limites por arquivo/sessão antes de receber dados.
3. Tratar nome como dado: remover caminhos absolutos, traversal, nomes reservados Windows e caracteres inválidos; impedir escrita por symlink/junction fora do destino autorizado.
4. Escrever em temporário exclusivo no destino escolhido, verificar hash e finalizar atomicamente quando possível. Não sobrescrever arquivo existente sem escolha explícita. O receptor local bounded e seus testes já cobrem essa etapa no desktop; integração ao destino escolhido pelo usuário permanece pendente.
5. Não carregar arquivo inteiro em RAM. Fazer backpressure e orçamento de banda/concorrência separado da mídia.
6. Usar canal autenticado associado à sessão, não outra porta aberta que aceite qualquer transfer_id.
7. Cancelar ao revogar/desconectar conforme política; remover apenas temporário comprovadamente pertencente à transferência. Nunca usar diretório do usuário como alvo recursivo.
8. Tratar disco cheio, permissão perdida, tamanho diferente do anunciado e hash incorreto; preservar arquivo anterior.
9. Não executar/abrir arquivo automaticamente. Retomar transferência é etapa posterior com revalidação do arquivo e offsets, não confiança cega no estado antigo.
10. Atualizar política de privacidade e permissões antes de disponibilizar.

## Aceite e testes

- [ ] Arquivo recebido tem hash/tamanho corretos e destino escolhido.
- [x] Traversal/nomes maliciosos são rejeitados pelo núcleo de validação.
- [x] Tamanho, ordem dos chunks, limite por chunk, cancelamento e hash final têm testes unitários.
- [ ] Cancelamento/queda deixam cleanup seguro e nenhum arquivo antigo alterado.
- [ ] Vídeo e input permanecem dentro do orçamento definido em T20.
- [ ] Memória é limitada independentemente do tamanho do arquivo.

Testar 0 bytes, tamanho acima do limite, nomes Unicode/longos, duplicados, disco cheio, desconexão no último chunk, hash inválido e receptor revogado.

Rollback desativa capability, cancela tarefas e preserva arquivos já concluídos; não limpar downloads do usuário.
