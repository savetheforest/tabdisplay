# T11 — Identidade, pareamento e revogação

## Identificação

- ID e título: T11 — Vincular identidade, canal e autorização de pareamento
- Status: Em validação
- Base: `d823e5b` + T01–T10 parciais + working tree de 29/09/2026

## Modelo de ameaça e resultado

- O beacon UDP é apenas descoberta de endpoint; `pc_id` recebido nele não autoriza token nem substitui pin.
- Loopback/ADB reverse deixou de ser tratado como prova física: todo HELLO sem token válido entra no pareamento, inclusive `127.0.0.1`. Isso evita confiar automaticamente em ADB sem fio ou outro processo local.
- O fingerprint TLS novo fica em memória até pareamento ou até o primeiro CONFIG de uma sessão autenticada por token. Um certificado diferente do pin preservado é recusado sem apagar pin/token e suspende retry automático pela Activity.
- Após pareamento, o tablet grava `pc_id`, token e o pin candidato sob a identidade do PC; USB reutiliza `pc_at_127.0.0.1` para não sobrescrever a identidade de dois PCs distintos.
- `forget_device` remove autorização persistida e encerra a sessão ativa correspondente no PC.
- Pareamento continua com código de uso único, TTL monotônico de 120 s e no máximo cinco tentativas; `pairing::cancel` limpa a pendência em erro.

## Arquivos

- `desktop/src-tauri/src/server.rs`: autenticação uniforme, revogação de sessão e sem bypass de loopback.
- `desktop/src-tauri/src/lib.rs`: `forget_device` revoga e desconecta.
- `desktop/src-tauri/src/pairing.rs`: comentários, política de token e commit transacional do pareamento.
- `desktop/src-tauri/src/settings.rs`: persistência atômica reutilizada pelo arquivo de identidade.
- `android/app/src/main/java/com/tabdisplay/MainActivity.kt`: pin candidato, mismatch sem apagar confiança, persistência por `pc_id` e USB.
- `PROTOCOL.md`: limitação explícita de loopback/USB.

## Validação

| Verificação | Ambiente/comando | Resultado |
|---|---|---|
| Pareamento/TTL/tentativas, persistência/revogação e servidor | `cargo test --locked` | Passou: 45 unitários, 1 ignorado, 1 integração; falhas de persistência não alteram autorização em memória |
| Cliente e persistência de identidade | `android\gradlew.bat testDebugUnitTest assembleDebug` | Passou no Gradle 9.7.1 |
| Certificado alterado, beacon falso e dois PCs USB | harness A/B com certificados reais | Pendente: não foi executada instalação/sessão física; não alegar proteção MITM total |
| Falha de persistência durante pareamento/revogação | testes Rust com pai inválido | Passou: `pairing_failure_is_reported_before_memory_authorization` e `revocation_failure_does_not_drop_in_memory_authorization`; nenhuma autorização é alterada quando a gravação falha |
| Revogação com sessão ativa | integração PC/tablet | Implementado por `disconnect_device`, execução física pendente |
| Arquivos de token e chave corrompidos/permissões | ambientes Windows/Android variados | Pendente; erro de leitura usa fallback recuperável, mas política de permissões ainda precisa matriz por OS |

## Limitações e reversão

O primeiro contato ainda depende da confirmação visual do código e do modelo trust-on-first-use; não é PAKE nem garantia contra MITM que retransmita o fluxo inicial. O armazenamento Android usa `SharedPreferences` e o PC usa JSON local, sem promessa de keystore/ACL endurecida nesta tarefa. Reversão não pode reintroduzir o bypass de loopback nem apagar silenciosamente a confiança em mismatch.
