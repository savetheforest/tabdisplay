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

Fase 1: espelha o monitor principal; toque = mouse.
