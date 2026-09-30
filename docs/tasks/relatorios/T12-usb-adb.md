# T12 — USB ADB com múltiplos aparelhos

## Identificação

- ID e título: T12 — Tornar a conexão ADB previsível e suportar vários tablets
- Status: Em validação
- Base: `d823e5b` + T01–T11 parciais + working tree de 29/09/2026

## Resultado

- O runner escolhe ADB do PATH ou bundled de forma determinística, usa argumentos separados, não abre janela de console no Windows e aplica timeout de cinco segundos; não usa `adb kill-server`.
- `adb devices -l` é parseado por estado e transporte. A UI recebe serial, modelo, `device`/`unauthorized`/`offline` e USB/ADB network; a instalação não transforma erro de autorização em “cabo ausente”.
- O reverse é executado como `adb -s <serial> reverse tcp:7070 tcp:7070` somente para aparelhos `device` reconhecidos como USB. Nenhum `--remove-all` ou reverse de outro serial é tocado.
- `install_apk` aceita serial explícito; sem serial só escolhe automaticamente quando há exatamente um aparelho. Com dois, exige seleção. Assinatura incompatível recebe orientação sem desinstalação/limpeza automática.
- A saída de falha é truncada na mensagem exibida; serial/modelo ficam apenas na UI local, não no export sanitizado de métricas.

## Arquivos

- `desktop/src-tauri/src/lib.rs`: runner, parser, polling por serial, reverse direcionado, estado no `status` e instalação seletiva.
- `desktop/src/main.js`, `desktop/src/index.html`: seleção explícita do aparelho e mensagens de estado.
- `docs/tasks/PROGRESSO.md`: evidência do único Redmi Pad 2 detectado neste ambiente.

## Validação

| Verificação | Ambiente/comando | Resultado |
|---|---|---|
| Parser fake 0/1/2, unauthorized/offline e transporte | `cargo test --locked` | Passou: teste cobre USB, ADB network, unauthorized e offline; 27 unitários, 1 ignorado, 1 integração |
| Runner/compilação desktop | `cargo test --locked` | Passou; timeout e argumentos compilam no Windows |
| Lista real | ADB bundled, serial `kfxkfin7nbpfq4q8` | Detectado exatamente um Redmi Pad 2 `25040RP0AE`, API 36, estado `device`; reverse/instalação não foram repetidos após a rejeição da aprovação de mutação |
| Dois tablets, troca de cabo e assinatura | hardware autorizado | Pendente; não instalar em todos nem desinstalar para contornar assinatura |
| macOS bundled ADB | Mac | Pendente; política é informar ausência/instalação necessária até T24 empacotar o binário adequado |

## Limitações e reversão

O estado `USB` é baseado no campo `usb:` de `adb devices -l`; ADB network é rotulado como tal, mas não é tratado como prova física. Reversão segura é parar o polling/reverse deste serial, sem retornar a comandos ADB ambíguos ou remoção global de mapeamentos.

Fonte operacional: [manual oficial do ADB](https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/docs/user/adb.1.md). A tentativa de leitura automatizada da página retornou 503 nesta execução; a sintaxe usada segue o contrato documentado no backlog.
