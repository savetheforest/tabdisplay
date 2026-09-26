#!/usr/bin/env node
// Licenças do TabDisplay (validadas offline pelo app com uma chave pública Ed25519 embutida).
//
//   node scripts/license.mjs keygen [arquivo]                 gera o par de chaves; guarda a privada em `arquivo`
//                                                             (padrão: ~/.tabdisplay/license-private.pem) e imprime
//                                                             a pública em hex (vai em desktop/src-tauri/src/license.rs)
//   node scripts/license.mjs issue "Nome" "email" [--key f]   imprime a licença (cole em Avançado > Licença no app)
//
// Formato: TDL1.<payload base64url>.<assinatura base64url>; o payload é o JSON {"name","email","issued"} e a
// assinatura Ed25519 cobre o texto "TDL1.<payload base64url>".
import { createPrivateKey, createPublicKey, generateKeyPairSync, sign } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";

const DEFAULT_KEY = join(homedir(), ".tabdisplay", "license-private.pem");
const [command, ...rest] = process.argv.slice(2);
const b64url = (buf) => Buffer.from(buf).toString("base64url");

function publicHex(privateKey) {
  const der = createPublicKey(privateKey).export({ type: "spki", format: "der" });
  return der.subarray(der.length - 32).toString("hex"); // raw 32-byte key at the end of the SPKI structure
}

if (command === "keygen") {
  const file = rest[0] ?? DEFAULT_KEY;
  if (existsSync(file)) {
    console.error(`${file} já existe: não sobrescrevo a chave privada (apague-o de propósito se quiser trocar).`);
    process.exit(1);
  }
  const { privateKey } = generateKeyPairSync("ed25519");
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, privateKey.export({ type: "pkcs8", format: "pem" }), { mode: 0o600 });
  console.log(`Chave privada: ${file} (guarde um backup; sem ela não dá para emitir licenças)`);
  console.log(`Chave pública (hex): ${publicHex(privateKey)}`);
} else if (command === "issue") {
  const args = [];
  let keyFile = DEFAULT_KEY;
  for (let i = 0; i < rest.length; i++) {
    if (rest[i] === "--key") keyFile = rest[++i];
    else args.push(rest[i]);
  }
  const [name, email] = args;
  if (!name || !email) {
    console.error('uso: node scripts/license.mjs issue "Nome" "email" [--key arquivo]');
    process.exit(1);
  }
  const privateKey = createPrivateKey(readFileSync(keyFile));
  const payload = b64url(JSON.stringify({ name, email, issued: new Date().toISOString().slice(0, 10) }));
  const signed = `TDL1.${payload}`;
  console.log(`${signed}.${b64url(sign(null, Buffer.from(signed), privateKey))}`);
} else {
  console.error("comandos: keygen [arquivo] | issue \"Nome\" \"email\" [--key arquivo]");
  process.exit(1);
}
