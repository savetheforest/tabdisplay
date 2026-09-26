#!/bin/sh
# Signs the macOS dev build with a stable self-signed identity ("TabDisplay Dev"), so macOS keeps the
# Screen Recording / Accessibility permissions across rebuilds (an ad-hoc signature changes every build,
# and macOS then asks again). Creates the identity on first run, in its own keychain with its own
# random password (the login keychain is not touched).
# Usage: scripts/sign-mac.sh [path/to/TabDisplay.app]
set -e
APP=${1:-"$(dirname "$0")/../desktop/src-tauri/target/release/bundle/macos/TabDisplay.app"}
DIR="$HOME/.tabdisplay-signing"
KC="$HOME/Library/Keychains/tabdisplay-dev.keychain-db"
mkdir -p "$DIR" && chmod 700 "$DIR"
[ -f "$DIR/pw" ] || { openssl rand -hex 24 > "$DIR/pw"; chmod 600 "$DIR/pw"; }
PW=$(cat "$DIR/pw")

if [ ! -f "$DIR/cert.pem" ]; then
    cat > "$DIR/req.cnf" <<EOF
[req]
distinguished_name=dn
x509_extensions=ext
prompt=no
[dn]
CN=TabDisplay Dev
[ext]
basicConstraints=critical,CA:false
keyUsage=critical,digitalSignature
extendedKeyUsage=critical,codeSigning
EOF
    openssl req -x509 -newkey rsa:2048 -nodes -keyout "$DIR/key.pem" -out "$DIR/cert.pem" -days 3650 -config "$DIR/req.cnf" 2>/dev/null
    openssl pkcs12 -export -inkey "$DIR/key.pem" -in "$DIR/cert.pem" -out "$DIR/id.p12" -passout "pass:$PW" -legacy 2>/dev/null \
        || openssl pkcs12 -export -inkey "$DIR/key.pem" -in "$DIR/cert.pem" -out "$DIR/id.p12" -passout "pass:$PW"
fi
[ -f "$KC" ] || security create-keychain -p "$PW" "$KC"
security unlock-keychain -p "$PW" "$KC"
if ! security find-identity -p codesigning "$KC" | grep -q "TabDisplay Dev"; then
    security import "$DIR/id.p12" -k "$KC" -P "$PW" -T /usr/bin/codesign >/dev/null
    security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$PW" "$KC" >/dev/null
    security list-keychains -d user -s $(security list-keychains -d user | tr -d '"' | grep -v tabdisplay-dev) "$KC"
fi
IDENTITY=$(security find-identity -p codesigning "$KC" | awk '/TabDisplay Dev/ {print $2; exit}')
codesign --force --deep --options runtime --timestamp=none --identifier com.tabdisplay.desktop \
    --keychain "$KC" --sign "$IDENTITY" "$APP"
codesign -d -r- "$APP" 2>&1 | tail -1
