# Relatório T24 — Distribuição e recuperação

Data: 29/09/2026  
Status: Em validação

## Entrega implementada

- A build de produção do Android define `TABDISPLAY_REQUIRE_RELEASE_SIGNING=true`; sem keystore a tarefa falha claramente. Builds locais sem essa variável continuam permitidos como debug/teste e exibem o aviso existente.
- `package.json`, `package-lock.json`, `Cargo.toml`, `tauri.conf.json`, `versionName` e `versionCode` permanecem coerentes em 0.2.0; o exemplo de verificação do README foi atualizado.
- O workflow Windows valida SHA-256 do Platform-Tools 37.0.1 antes de extrair e embutir o ADB. O URL oficial é móvel, mas qualquer mudança do arquivo faz o CI falhar fechado. O hash do driver VDD 25.7.23 já era fixado e foi preservado.
- Hooks NSIS deixam de continuar após falhas: firewall, driver, criação/configuração/recuperação do serviço, uninstall e remoção da regra são checados. O serviço é esperado por até 20 s em estado parado/running.
- A regra de firewall do instalador fica restrita aos perfis `private,domain`; não desativa o firewall nem abre o roteador. Redes públicas exigem ação explícita do usuário.
- A desinstalação continua chamando o comando do driver, que só remove o dispositivo quando existe o marcador `installed-by-tabdisplay`; driver preexistente não é removido.
- O teste existente de assinatura do updater permanece no workflow desktop e continua recusando payload adulterado/assinatura estrangeira.

## Arquivos principais

- `android/app/build.gradle.kts`
- `.github/workflows/build.yml`
- `desktop/src-tauri/windows/hooks.nsh`
- `desktop/package-lock.json`
- `README.md`

## Validação

- O build Android debug e os testes JVM passaram localmente antes desta regra de produção; o caminho de release sem segredo não foi executado como distribuição.
- A regra de assinatura obrigatória foi implementada sem gerar nova chave nem acessar segredos.
- O hash do arquivo oficial Platform-Tools 37.0.1 foi conferido localmente: `45F4D63113E895EBDE0C90F194099A4676B6AC653BD28D54314A9E022BBC1A99`.
- CI remoto, VM de instalação/upgrade e assinatura/notarização de Windows/macOS não foram executados.

## Pendências e rollback

Faltam snapshots de instalação limpa, upgrade, cancelamento, disco cheio, serviço/firewall/driver com falha, rollback parcial e verificação de SmartScreen/Gatekeeper. Não houve publicação, envio à loja, geração de chave ou mutação de instalação do usuário. O rollback desta tarefa é remover apenas o gate/workflow/hooks novos; não remover driver ou dados de terceiros.
