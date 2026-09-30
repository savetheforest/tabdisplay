# TabDisplay

Tablet Android como monitor do PC. `desktop/` = Tauri (Rust), `android/` = app Kotlin. Protocolo: [PROTOCOL.md](PROTOCOL.md).

## Planejamento técnico

O [backlog documentado](docs/tasks/README.md) reúne tarefas de estabilidade, FPS, USB, mobile, segurança,
distribuição e recursos futuros, com dependências, critérios de aceite e instruções de execução para IA.
Comece pelo [guia de execução](docs/tasks/GUIA-EXECUCAO.md) antes de implementar uma tarefa.

## Instalar (Windows)
Rode `TabDisplay_<versão>_x64-setup.exe`. O instalador (pede admin uma vez):
- instala o [Virtual Display Driver](https://github.com/VirtualDrivers/Virtual-Display-Driver) (MIT) com as resoluções do app,
  ou, se ele já existir, só grava as resoluções e reinicia o driver;
- libera o TabDisplay no firewall do Windows;
- traz o `adb` e o app do tablet (`tabdisplay.apk`).

No tablet: ligue a **depuração USB**, conecte o cabo e clique em **Instalar app no tablet** na janela do PC.
Depois disso o tablet lista uma **Conexão local pelo ADB**. Na primeira conexão ela ainda pede o código de
pareamento; depois o token e o certificado ficam guardados neste tablet para esse PC.

Sem mexer em opções do desenvolvedor: instale o app pelo APK (compartilhado por algum outro meio) e,
já com o cabo conectado, ligue **Compartilhar internet pelo USB** nas configurações do tablet. O PC
aparece na lista como se fosse Wi‑Fi (com o mesmo pareamento por código).

## Gerar o instalador
Precisa de `adb` no PATH (`winget install Google.PlatformTools`) e do driver baixado
(`winget install --id=VirtualDrivers.Virtual-Display-Driver -e`; o instalador do TabDisplay é quem instala de fato).
```
powershell -File scripts/prepare-resources.ps1
cd desktop && npm install && npx tauri build
```
Sai em `desktop/src-tauri/target/release/bundle/nsis/`.

### Assinar o instalador (Windows)
O Tauri chama `scripts/sign-windows.ps1` para o `tabdisplay.exe` e para o instalador. O certificado vem do
ambiente, nunca do repositório:
- `TABDISPLAY_SIGN_PFX` + `TABDISPLAY_SIGN_PFX_PASSWORD`: certificado em arquivo `.pfx` (OV); ou
- `TABDISPLAY_SIGN_THUMBPRINT`: certificado no repositório do Windows (token USB de um certificado EV).

Sem nenhum dos dois o build sai sem assinatura (aviso no log), como sempre. Precisa do `signtool` (Windows SDK).
Confira: `signtool verify /pa /v TabDisplay_0.2.0_x64-setup.exe`.

Qual certificado comprar: um EV tira o aviso do SmartScreen na hora; um OV comum assina, mas o SmartScreen
ainda avisa até o app ganhar reputação. Vale ver também o Azure Trusted Signing (assinatura na nuvem, sem
token). Nos três casos o `signCommand` pode apontar para outro comando: é só ajustar `bundle.windows.signCommand`
em `desktop/src-tauri/tauri.windows.conf.json`.

## macOS (Apple Silicon, macOS 14+)
Não precisa de driver: o monitor virtual usa a `CGVirtualDisplay` do sistema e existe só enquanto um
tablet está conectado. Na primeira execução o macOS pede **Gravação de Tela**, **Acessibilidade**
(o toque do tablet vira clique) e **Rede Local** (para o tablet achar o Mac).

Gerar o `.dmg` (num Mac, com Rust e `cargo install tauri-cli --version "^2"`):
```
cp <app-release.apk> desktop/src-tauri/resources/tabdisplay.apk
sh scripts/sign-mac.sh          # 1ª vez: cria a identidade "TabDisplay Dev" (chaveiro próprio)
cd desktop && CI=true APPLE_SIGNING_IDENTITY="TabDisplay Dev" cargo tauri build --bundles app,dmg
```
A assinatura estável faz o macOS manter as permissões entre builds. Ela é autoassinada: em outros Macs
o Gatekeeper pede clique direito → Abrir. Para distribuir de verdade, use um Developer ID da Apple e notarize:

### Developer ID e notarização
1. Conta no Apple Developer Program → Certificates → **Developer ID Application** → instale o certificado no chaveiro.
2. Crie uma senha específica de app em appleid.apple.com → Segurança.
3. Rode (o script confere as variáveis, gera o `.app` e o `.dmg`, e verifica com `spctl` e `stapler`):
```
export APPLE_SIGNING_IDENTITY="Developer ID Application: Seu Nome (ABCDE12345)"
export APPLE_ID="voce@exemplo.com" APPLE_PASSWORD="senha-de-app" APPLE_TEAM_ID="ABCDE12345"
scripts/notarize-mac.sh
```
O Tauri assina com hardened runtime, envia ao `notarytool`, espera o resultado e grampeia o ticket no app.
Pronto quando `spctl -a -vv TabDisplay.app` disser `accepted` / `source=Notarized Developer ID`. As permissões
de Gravação de Tela e Acessibilidade ficam presas à identidade de assinatura: ao trocar de “TabDisplay Dev”
para o Developer ID o macOS pede as permissões de novo uma vez.

## APK de release assinado

Toda máquina que gera o APK de release precisa da **mesma** keystore, senão o Android recusa atualizar um APK por cima do outro (assinaturas diferentes). A keystore e as senhas nunca entram no repositório.

Uma vez, numa máquina de confiança:

```bash
keytool -genkeypair -keystore ~/.tabdisplay/android-release.jks -alias tabdisplay -keyalg RSA -keysize 4096 -validity 10000
```

O Gradle procura a chave em variáveis de ambiente (CI: `TABDISPLAY_KEYSTORE`, `TABDISPLAY_KEYSTORE_PASSWORD`, `TABDISPLAY_KEY_ALIAS`, `TABDISPLAY_KEY_PASSWORD`) ou em `~/.tabdisplay/android-release.properties`:

```properties
storeFile=C:/Users/voce/.tabdisplay/android-release.jks
storePassword=…
keyAlias=tabdisplay
keyPassword=…
```

Depois `cd android && ./gradlew assembleRelease`. Confira o certificado com `apksigner verify --print-certs app/build/outputs/apk/release/app-release.apk`: o SHA-256 tem que ser o mesmo em qualquer máquina. Guarde a keystore e as senhas em backup (no GitHub Actions, como secrets): sem elas não há como atualizar o app já instalado nem publicar atualizações na Play Store.

Trocar da chave de debug para esta faz o Android recusar a atualização de instalações antigas (desinstale o app do tablet uma vez).

## Ícones
`python scripts/make-icon.py` desenha a logo; `npx tauri icon` gera os tamanhos do desktop e
`python scripts/make-android-icon.py` gera o ícone adaptativo do Android.

## Desenvolver
```
cd desktop && npm run tauri dev
cd android && ./gradlew installDebug
```

## Atualização automática (Windows e Mac)
O app procura, uns segundos depois de abrir (e em Avançado → Atualizações), o `latest.json` da última release
do GitHub (`plugins.updater.endpoints` em `desktop/src-tauri/tauri.conf.json`). Achando uma versão maior, mostra
um aviso em Início; “Instalar e reiniciar” baixa, **confere a assinatura minisign contra a chave pública embutida
no app** e só então instala (assinatura errada ou arquivo adulterado = a atualização é recusada; há teste
para isso em `desktop/src-tauri/tests/update_signature.rs`). No Mac o `.app` é substituído no lugar e mantém a
mesma identidade de assinatura, então as permissões de Gravação de Tela/Acessibilidade continuam valendo.

Chaves (uma vez): `cd desktop && npx tauri signer generate -w ~/.tabdisplay/updater.key`. A **chave privada e a senha ficam
fora do repositório** (backup!); a chave pública vai em `plugins.updater.pubkey`. Perdeu a privada: só um instalador
novo, com outra chave pública, alcança os usuários.

Publicar uma versão: aumente a versão em `desktop/src-tauri/tauri.conf.json`, `Cargo.toml` e `desktop/package.json`,
commite na `main` e empurre uma tag `vX.Y.Z` igual (`git tag v0.2.1 && git push origin v0.2.1`). O job `publish` do
`.github/workflows/build.yml` builda Windows e Mac assinados (usa os secrets `TAURI_SIGNING_PRIVATE_KEY*` já configurados),
monta o `latest.json` com `scripts/make-latest-json.mjs` e publica tudo numa GitHub Release — sozinho, sem passo manual.
O app só enxerga a release se o endereço for público (repositório público, ou troque `endpoints` por um endpoint próprio).

Rodando `make-latest-json.mjs` manualmente (build feito à mão fora do CI, ex. para testar num Mac local):
```
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tabdisplay/updater.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=...
cd desktop && cargo tauri build --bundles app,dmg --config src-tauri/tauri.release.conf.json
node ../scripts/make-latest-json.mjs 0.2.1 https://github.com/<dono>/tabdisplay/releases/download/v0.2.1 --notes "o que mudou"
```

## Build automático (GitHub Actions)
A cada merge na main, `.github/workflows/build.yml` gera, como artefatos da execução (aba Actions → a execução → Artifacts, 30 dias):
- `tabdisplay-android`: `app-release.apk` e `app-release.aab`;
- `tabdisplay-windows`: `TabDisplay_<versão>_x64-setup.exe` (com o APK, o adb e o driver do monitor virtual embutidos; roda `cargo test` antes);
- `tabdisplay-macos`: `TabDisplay_<versão>_aarch64.dmg` (Apple Silicon).

Também dá para rodar sob demanda (Actions → Build → Run workflow). Nos pull requests roda só o `ci.yml` (compila e testa).
Numa tag `vX.Y.Z` roda mais um job, `publish`, que junta os artefatos do Windows e do Mac numa GitHub Release pública com
o `latest.json` do atualizador (veja "Atualização automática" acima).
Sem segredos o build sai igual, mas sem assinatura: o APK usa a chave de debug do runner (serve para testar; não atualiza um app instalado com a chave de release),
o instalador do Windows fica sem assinatura e o app do Mac sem Developer ID. Para assinar de verdade, cadastre em Settings → Secrets and variables → Actions:

| Segredo | Conteúdo |
|---|---|
| `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD` | `base64 -w0 ~/.tabdisplay/android-release.jks` e os valores de `android-release.properties` |
| `WINDOWS_CERT_PFX_BASE64`, `WINDOWS_CERT_PASSWORD` | certificado de code signing (.pfx) em base64 e a senha |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Developer ID (.p12 em base64) e os dados da notarização |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | conteúdo de `~/.tabdisplay/updater.key` e a senha (gera os pacotes do atualizador) |
| `SENTRY_DSN` | liga o relatório de erros no PC e no Android |

## Licença (modo estender)

Sem licença o app só espelha; estender exige uma licença válida (Avançado → Licença). A validação é offline: a licença é um texto assinado com Ed25519 e o app traz só a chave pública (`desktop/src-tauri/src/license.rs`).

Do lado de quem vende:

```bash
node scripts/license.mjs keygen                        # uma vez: guarda a chave privada em ~/.tabdisplay e imprime a pública
node scripts/license.mjs issue "Nome" "email@exemplo.com"  # imprime a licença para enviar ao cliente
```

A chave privada nunca entra no repositório. Se trocar o par de chaves, atualize `PUBLIC_KEY` em `license.rs` (licenças antigas deixam de valer). O link do botão “Comprar licença” é `BUY_URL` no mesmo arquivo.

## Relatório de erros (Sentry)

Desligado por padrão. Para ligar no app do PC, defina o DSN ao compilar (`TABDISPLAY_SENTRY_DSN=https://…@…ingest.sentry.io/… cargo tauri build`) ou no ambiente ao abrir o app. Panics de qualquer thread (o release aborta o processo, então o app espera o envio) e falhas recuperáveis (captura, encoder de hardware, driver) viram eventos; não vão IP, nome do PC/tablet, códigos nem tokens.

No Android é igual: `TABDISPLAY_SENTRY_DSN=… ./gradlew assembleRelease` (sem DSN o Sentry nem inicia). Crashes e erros inesperados de conexão (falha de TLS, por exemplo) chegam com breadcrumbs do fluxo de conexão, sem token, nome ou endereço.

## Monitor virtual (modo estender)
O app grava as resoluções em `C:\VirtualDisplayDriver\vdd_settings.xml`, que o driver só lê ao iniciar.
O instalador já faz esse reinício; **Reiniciar driver** na janela do app fica para casos raros
(tablet com um tamanho novo, ou driver parado). Sem o driver, o app só espelha.
Comandos do executável, com admin: `tabdisplay.exe --install-driver | --restart-driver | --uninstall-driver`
(o uninstall só remove o driver se foi o TabDisplay que o instalou).

Limites conhecidos:
- Nunca recarregue o driver pelo pipe (`RELOAD_DRIVER`/`SETDISPLAYCOUNT`): na versão 25.7 ele crasha (Código 43).
- Com mais de ~100 modos no XML (resoluções × `g_refresh_rate`) o driver não cria o monitor.
- O APK do instalador é assinado com a keystore de release (veja “APK de release assinado”); sem ela, cai na chave de debug da máquina.
