# TabDisplay

Tablet Android como monitor do PC. `desktop/` = Tauri (Rust), `android/` = app Kotlin. Protocolo: [PROTOCOL.md](PROTOCOL.md).

## Instalar (Windows)
Rode `TabDisplay_<versão>_x64-setup.exe`. O instalador (pede admin uma vez):
- instala o [Virtual Display Driver](https://github.com/VirtualDrivers/Virtual-Display-Driver) (MIT) com as resoluções do app,
  ou, se ele já existir, só grava as resoluções e reinicia o driver;
- libera o TabDisplay no firewall do Windows;
- traz o `adb` e o app do tablet (`tabdisplay.apk`).

No tablet: ligue a **depuração USB**, conecte o cabo e clique em **Instalar app no tablet** na janela do PC.
Depois disso o tablet lista o PC sozinho: toque em **USB (cabo)** ou no nome do PC (Wi‑Fi).

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
o Gatekeeper pede clique direito → Abrir (para distribuir de verdade, use um Developer ID da Apple).

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
