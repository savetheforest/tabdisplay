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
