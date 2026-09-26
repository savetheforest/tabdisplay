#!/usr/bin/env node
// Monta o latest.json do atualizador do Tauri a partir do que o `tauri build` gerou (os arquivos .sig).
// Cada máquina gera o seu lado (Windows e Mac); use --merge para juntar no mesmo arquivo.
//
//   TAURI_SIGNING_PRIVATE_KEY=... cargo tauri build --config src-tauri/tauri.release.conf.json   (gera os .sig)
//   node scripts/make-latest-json.mjs 0.2.0 https://github.com/savetheforest/tabdisplay/releases/download/v0.2.0 \
//        [--notes "o que mudou"] [--bundle desktop/src-tauri/target/release/bundle] [--merge latest.json] [--out latest.json]
//
// Suba o latest.json e os instaladores/.tar.gz para a mesma release; o app procura em
// https://github.com/<repo>/releases/latest/download/latest.json (plugins.updater.endpoints).
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";

const args = process.argv.slice(2);
const flag = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const [version, baseUrl] = args;
if (!version || !baseUrl || version.startsWith("--")) {
  console.error("uso: node scripts/make-latest-json.mjs <versão> <url-base-dos-arquivos> [--notes t] [--bundle dir] [--merge arquivo] [--out arquivo]");
  process.exit(1);
}
const bundle = flag("--bundle", "desktop/src-tauri/target/release/bundle");
const out = flag("--out", "latest.json");
const mergeFrom = flag("--merge", null);

// O nome do arquivo diz a plataforma: instalador NSIS (Windows) ou .app.tar.gz (Mac, Apple Silicon).
const platformOf = (file) => (/-setup\.exe$/.test(file) ? "windows-x86_64" : /\.app\.tar\.gz$/.test(file) ? "darwin-aarch64" : null);

function* walk(dir) {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) yield* walk(path);
    else yield path;
  }
}

const latest = mergeFrom && existsSync(mergeFrom) ? JSON.parse(readFileSync(mergeFrom, "utf8")) : { platforms: {} };
if (latest.version && latest.version !== version) {
  console.error(`o arquivo a mesclar é da versão ${latest.version}, não ${version}`);
  process.exit(1);
}
latest.version = version;
latest.notes = flag("--notes", latest.notes ?? `TabDisplay ${version}`);
latest.pub_date = new Date().toISOString();

let found = 0;
for (const path of walk(bundle)) {
  if (!path.endsWith(".sig")) continue;
  const artifact = path.slice(0, -4);
  const platform = platformOf(artifact);
  if (!platform || !existsSync(artifact)) continue;
  latest.platforms[platform] = {
    signature: readFileSync(path, "utf8").trim(),
    url: `${baseUrl.replace(/\/$/, "")}/${encodeURIComponent(basename(artifact))}`,
  };
  found++;
}
if (!found) {
  console.error(`nenhum .sig de instalador em ${bundle}: gere o build com TAURI_SIGNING_PRIVATE_KEY e createUpdaterArtifacts`);
  process.exit(1);
}
writeFileSync(out, JSON.stringify(latest, null, 2) + "\n");
console.log(`${out}: versão ${version}, plataformas ${Object.keys(latest.platforms).join(", ")}`);
