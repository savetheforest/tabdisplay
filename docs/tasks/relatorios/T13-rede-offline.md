# T13 — Descoberta, interfaces e operação offline

## Identificação

- ID e título: T13 — Mostrar e selecionar corretamente caminhos locais, inclusive USB
- Status: Em validação
- Base: `d823e5b` + T01–T12 parciais + working tree de 29/09/2026

## Resultado

- O status do PC enumera IPv4 não-loopback locais via `if_addrs`, podendo mostrar Wi-Fi, Ethernet, VPN/virtual e outras interfaces; não conecta UDP a `8.8.8.8` nem exige internet para escolher o endereço exibido.
- O beacon continua sendo apenas descoberta. `pc_id` não autoriza conexão; pin/token são validados no TLS/HELLO conforme T11.
- O endpoint `127.0.0.1` no Android é apresentado como “conexão local pelo ADB”; não é chamado de USB nem de tethering sem evidência do transporte. O PC permite selecionar o serial USB separadamente no instalador.
- Discovery usa `SystemClock.elapsedRealtime()` para expiração, agrupa por endpoint, valida `id`/nome do beacon, tolera bind de UDP falho e mantém o probe local/manual disponível. `start`/`stop` são idempotentes por instância.
- A conexão manual continua na porta fixa 7070 e aceita hostname/IP sem sintaxe de porta inventada; endereços IPv6/mDNS continuam fora do suporte anunciado.

## Arquivos

- `desktop/src-tauri/src/lib.rs`: endereços locais no status, sem destino público.
- `android/app/src/main/java/com/tabdisplay/Discovery.kt`: socket opcional, lifecycle, expiração monotônica, validação e rótulo local.
- `android/app/src/main/java/com/tabdisplay/MainActivity.kt`, strings: UI sem alegação de cabo físico.

## Validação

| Verificação | Ambiente/comando | Resultado |
|---|---|---|
| Cliente Android e falha de bind/lifecycle compiláveis | `android\gradlew.bat testDebugUnitTest assembleDebug` | Passou no Gradle 9.7.1 |
| PC e enumeração de interfaces | `cargo test --locked` | Passou: 27 unitários, 1 ignorado, 1 integração; `status` compila usando `if_addrs` |
| Operação sem internet, múltiplos IPs e AP bloqueando broadcast | rede controlada | Pendente; não desabilitei interfaces nem alterei rede do usuário |
| Wi-Fi + ADB local simultâneos e troca de IP | tablet físico atualizado | Pendente pelo bloqueio da instalação/mutação |
| IPv6/mDNS | laboratório dedicado | Não implementado; suporte atual é IPv4/manual e isso está declarado |

## Limitações e reversão

O endereço exibido é uma lista de interfaces, não uma garantia de rota utilizável ou de que o PC responderá por todas elas. A reversão mantém descoberta IPv4/manual e autenticação; não reintroduz requisito de internet nem classifica loopback como USB por faixa de IP.
