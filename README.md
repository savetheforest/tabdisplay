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

## Monitor virtual (modo estender)
O app grava as resoluções em `C:\VirtualDisplayDriver\vdd_settings.xml`, que o driver só lê ao iniciar.
O instalador já faz esse reinício; **Reiniciar driver** na janela do app fica para casos raros
(tablet com um tamanho novo, ou driver parado). Sem o driver, o app só espelha.
Comandos do executável, com admin: `tabdisplay.exe --install-driver | --restart-driver | --uninstall-driver`
(o uninstall só remove o driver se foi o TabDisplay que o instalou).

Limites conhecidos:
- Nunca recarregue o driver pelo pipe (`RELOAD_DRIVER`/`SETDISPLAYCOUNT`): na versão 25.7 ele crasha (Código 43).
- Com mais de ~100 modos no XML (resoluções × `g_refresh_rate`) o driver não cria o monitor.
- O APK é assinado com a chave de debug da máquina que gerou o instalador.
