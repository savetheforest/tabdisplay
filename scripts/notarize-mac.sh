#!/bin/sh
# Gera o TabDisplay.app e o .dmg assinados com Developer ID e notarizados pela Apple, e confere o resultado.
# O Tauri faz a assinatura (hardened runtime), o envio ao notarytool e o staple do app (o dmg leva o app grampeado) quando estas variáveis existem:
#   APPLE_SIGNING_IDENTITY   "Developer ID Application: Seu Nome (ABCDE12345)"  (do chaveiro)
#   APPLE_ID                 e-mail do Apple ID da conta de desenvolvedor
#   APPLE_PASSWORD           senha específica de app (appleid.apple.com > Segurança), não a senha da conta
#   APPLE_TEAM_ID            ID da equipe (10 caracteres)
# Uso, num Mac com Rust e `cargo install tauri-cli --version "^2"`:
#   cp <app-release.apk> desktop/src-tauri/resources/tabdisplay.apk
#   scripts/notarize-mac.sh
set -e
for v in APPLE_SIGNING_IDENTITY APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
    eval "[ -n \"\${$v}\" ]" || { echo "falta a variável $v (veja o topo deste script)" >&2; exit 1; }
done
security find-identity -v -p codesigning | grep -qF "$APPLE_SIGNING_IDENTITY" \
    || { echo "identidade '$APPLE_SIGNING_IDENTITY' não está no chaveiro (security find-identity -v -p codesigning)" >&2; exit 1; }

ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT/desktop"
CI=true cargo tauri build --bundles app,dmg

BUNDLE="$ROOT/desktop/src-tauri/target/release/bundle"
APP="$BUNDLE/macos/TabDisplay.app"
DMG=$(ls "$BUNDLE"/dmg/*.dmg | head -1)

echo "== assinatura"
codesign --verify --deep --strict --verbose=2 "$APP"
codesign -dv "$APP" 2>&1 | grep -E "Authority|TeamIdentifier|flags"
echo "== Gatekeeper (o app precisa sair 'accepted', source=Notarized Developer ID)"
spctl -a -vv "$APP"
echo "== ticket de notarização grampeado no app (funciona offline)"
xcrun stapler validate "$APP"
echo "pronto: $DMG"
