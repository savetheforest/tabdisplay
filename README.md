# TabDisplay

Tablet Android como monitor do PC. `desktop/` = Tauri (Rust), `android/` = app Kotlin. Protocolo: [PROTOCOL.md](PROTOCOL.md).

## Rodar
PC (Windows):
```
cd desktop && npm install && npm run tauri dev
```
Tablet:
```
cd android && ./gradlew installDebug
```
- **USB:** depuração USB ligada no tablet → "Conectar por USB" no PC → "Conectar por USB" no tablet.
- **Wi‑Fi:** digite no tablet o IP mostrado no PC (libere a porta 7070 no firewall do Windows).

## Monitor virtual (modo estender)
Precisa do [Virtual Display Driver](https://github.com/VirtualDrivers/Virtual-Display-Driver):
`winget install --id=VirtualDrivers.Virtual-Display-Driver -e`, depois abrir o VDD Control e clicar em Install.
O app grava as resoluções em `C:\VirtualDisplayDriver\vdd_settings.xml`; o driver só as lê ao iniciar,
então use **Reiniciar driver** na janela do app quando pedir. Sem o driver, o app só espelha.

Limites conhecidos:
- Nunca recarregue o driver pelo pipe (`RELOAD_DRIVER`/`SETDISPLAYCOUNT`): na versão 25.7 ele crasha (Código 43).
- Com mais de ~100 modos no XML (resoluções × `g_refresh_rate`) o driver não cria o monitor.
- O cursor do mouse não aparece no vídeo.
