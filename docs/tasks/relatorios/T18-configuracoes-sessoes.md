# T18 — Configurações por tablet e persistência

## Identificação

- ID e título: T18 — Isolar configurações por tablet e sincronizar o estado da UI
- Status: Em validação
- Base: `d823e5b` + T01–T17 parciais + working tree de 29/09/2026
- Dependências verificadas: T01, T02 e T11 implementados localmente

## Resultado

O perfil enviado pelo tablet deixou de editar `settings.json`/`Settings` global.
Cada sessão autenticada tem `Shared.profile`; `PROFILE` atualiza somente essa
sessão, marca seu rebuild e publica o perfil em `SessionInfo`. O desktop mostra
os perfis junto aos nomes quando há mais de um tablet, evitando que o polling
regrave o perfil global a partir de `sessions.first`.

As configurações do PC continuam sendo o default global para novas sessões e os
controles de vídeo da UI ainda são globais. Essa limitação é intencional nesta
etapa: não foi inventado um seletor de “tablet alvo” para controles que a UI
atual não oferece.

`settings::set` agora classifica mudanças: somente modo/perfil/resolução/
posição/monitor/FPS/bitrate/encoder incrementam a versão que reconstrói vídeo.
Onboarding, áudio, toque e modo de toque não bumpam essa revisão. A persistência
escreve um arquivo temporário e substitui o arquivo configurado, com backup
transitório no Windows; falhas retornam erro ao comando Tauri antes de publicar
o novo estado em memória.

## Contratos e limites

- Identidade da sessão continua sendo `device_id` autenticado de T11; nome não é
  chave.
- `PROFILE` não aceita `custom` do tablet e não altera licença/default global.
- O perfil efetivo fica visível por sessão; métricas continuam sem IDs/endpoints
  persistentes no relatório exportado.
- Alteração global do desktop ainda pode reconstruir sessões conforme o modelo
  atual; a separação de alvo para controles globais é trabalho posterior.

## Validação

| Verificação/cenário | Ambiente e comando | Resultado observado | Artefato |
|---|---|---|---|
| Backend, sessão e persistência compiláveis | `cargo test --locked` em Windows | Passou: 45 unitários, 1 ignorado, 1 integração; inclui classificação de revisão e substituição de arquivo em diretório temporário | Cargo; `settings::tests::persistence_replaces_existing_file_without_leftover_staging_files` |
| Tablet envia PROFILE | teste físico com dois tablets | Não executado; atualização/abertura do APK no tablet continua bloqueada | pendente |
| Dois perfis simultâneos e stats por sessão | laboratório com dois aparelhos | Não executado; implementação deixa a identidade no array `sessions` e não usa `first` para o perfil | pendente |
| onboarding/áudio/toque sem rebuild | teste de UI + sessão ativa | Backend classifica sem bump; exercício físico pendente | pendente |
| substituição atômica e arquivos temporários | teste Rust em diretório temporário | Passou: duas gravações, incluindo substituição de arquivo existente, preservaram JSON válido e não deixaram `.tmp`/`.bak` | `desktop/src-tauri/src/settings.rs` |
| disco sem escrita/JSON truncado/concorrência | harness de persistência | Ainda não executado; falha de escrita agora é retornada e carga inválida continua caindo em defaults validados | VM/harness adicional |

Não foi declarado que a UI já permita editar uma preferência individual por
tablet. A camada de sessão e o contrato de PROFILE estão prontos; o seletor de
alvo e mudanças concorrentes da UI permanecem pendentes.

## Operação e reversão

O rollback pode voltar a tratar o perfil como global removendo `Shared.profile`
e usando o default, sem alterar tokens/pareamentos. O arquivo `settings.json`
mantém os campos existentes; não há migração destrutiva.

Próximo passo exato: ler `docs/tasks/T19-monitores-driver.md`, revisar o dono do
monitor por sessão e impedir fallback silencioso para outro display antes de
alterar o driver.
