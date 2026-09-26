const { invoke } = window.__TAURI__.core;
const $ = (id) => document.getElementById(id);
const $$ = (sel) => [...document.querySelectorAll(sel)];

const PROFILES = {
  performance: "Metade da resolução do tablet, 60 fps, 10 Mbps. Mais leve para Wi‑Fi fraco.",
  balanced: "Resolução do tablet, 60 fps, 20 Mbps.",
  quality: "Resolução do tablet, 60 fps, 40 Mbps. Mais nítido, pede uma boa conexão.",
  auto: "Começa no equilibrado e ajusta sozinho: baixa a qualidade se a rede piorar e sobe de volta quando melhora.",
  custom: "Você escolhe resolução, quadros por segundo e taxa de dados.",
};
const DRIVER = {
  ok: "Pronto. Aparece só enquanto um tablet está conectado.",
  "sem-servico": "O serviço do TabDisplay está parado. Reinstale o app para estender a tela.",
  ausente: "Driver não instalado: só dá para espelhar. Reinstale o app.",
  "sem-permissao": "Falta permissão: em Ajustes do Sistema → Privacidade e Segurança, ligue o TabDisplay em Gravação de Tela e em Acessibilidade.",
};

let settings;
let dismissedCode = null;

// ---- navigation ----
for (const button of $$("nav button[data-page]")) {
  button.onclick = () => {
    for (const b of $$("nav button[data-page]")) b.removeAttribute("aria-current");
    button.setAttribute("aria-current", "page");
    for (const s of $$("main > section")) s.hidden = s.id !== `page-${button.dataset.page}`;
    if (button.dataset.page === "devices") loadPaired();
  };
}

// ---- settings <-> controls ----
function read(el) {
  const key = el.dataset.setting;
  if (el.type === "checkbox") return el.checked;
  if (el.type === "range") return Number(el.value);
  if (key === "mirror_monitor") return el.value || null;
  if (key === "resolution") return el.value === "native" ? null : el.value.split("x").map(Number);
  return el.value;
}

function render() {
  for (const seg of $$(".seg[data-setting]")) {
    for (const b of seg.querySelectorAll("button")) {
      b.setAttribute("aria-pressed", String(b.dataset.value === String(settings[seg.dataset.setting])));
    }
  }
  for (const el of $$("select[data-setting], input[data-setting]")) {
    const v = settings[el.dataset.setting];
    if (el.type === "checkbox") el.checked = v;
    else if (el.dataset.setting === "resolution") el.value = v ? v.join("x") : "native";
    else el.value = v ?? "";
  }
  const show = { extend: settings.mode === "extend", mirror: settings.mode === "mirror", custom: settings.profile === "custom", touch: settings.touch };
  for (const row of $$("[data-show]")) row.hidden = !show[row.dataset.show];
  $("bitrate-out").textContent = `${settings.bitrate_mbps} Mbps`;
  $("profile-desc").textContent = $("profile-desc-2").textContent = PROFILES[settings.profile];
  $("touch-state").textContent = settings.touch ? "Ligado" : "Desligado";
}

async function save(patch) {
  settings = { ...settings, ...patch };
  render();
  await invoke("set_settings", { settings });
}

for (const seg of $$(".seg[data-setting]")) {
  for (const b of seg.querySelectorAll("button")) {
    b.onclick = () => save({ [seg.dataset.setting]: "number" in seg.dataset ? Number(b.dataset.value) : b.dataset.value });
  }
}
for (const el of $$("select[data-setting], input[data-setting]")) {
  el.onchange = () => save({ [el.dataset.setting]: read(el) });
  if (el.type === "range") el.oninput = () => ($("bitrate-out").textContent = `${el.value} Mbps`);
}

function option(value, text) {
  const o = document.createElement("option");
  o.value = value;
  o.textContent = text;
  return o;
}

// ---- live status ----
function setBadge(kind, text) {
  const badge = $("hero-badge");
  badge.className = `badge ${kind}`;
  badge.querySelector("use").setAttribute("href", { good: "#i-ok", warn: "#i-warn", bad: "#i-warn", wait: "#i-wait" }[kind]);
  $("hero-detail").textContent = text;
}

async function refresh() {
  const s = await invoke("status");
  const connected = Boolean(s.session);
  if (s.profile !== settings.profile) {
    // The tablet changed the quality preset.
    settings.profile = s.profile;
    render();
  }
  $("hero").classList.toggle("connected", connected);

  if (s.pairing) {
    $("hero-title").textContent = `Pareando com ${s.pairing.device}`;
    setBadge("warn", "Digite no tablet o código mostrado na tela");
    $("pair-device").textContent = s.pairing.device;
    $("pair-code").textContent = s.pairing.code;
    if (!$("pair-dialog").open && dismissedCode !== s.pairing.code) $("pair-dialog").showModal();
  } else {
    if ($("pair-dialog").open) $("pair-dialog").close();
    if (connected) {
      $("hero-title").textContent = `Conectado a ${s.session}`;
      const fallback = s.status.includes("falhou");
      if (s.stats?.reduced) setBadge("warn", "Rede lenta: reduzindo a qualidade automaticamente");
      else setBadge(fallback ? "warn" : "good", s.status.replace(/ ·.*$/, ""));
    } else {
      $("hero-title").textContent = "Aguardando tablet";
      setBadge("wait", "No tablet, abra o TabDisplay e toque no nome deste PC");
    }
  }

  $("tiles").hidden = !s.stats;
  if (s.stats) {
    const t = s.stats;
    $("t-res").textContent = `${t.width}×${t.height}`;
    $("t-res-s").textContent = settings.mode === "extend" ? "monitor virtual" : "espelhando";
    $("t-fps").textContent = Math.round(t.fps);
    $("t-fps-s").textContent = `tablet exibe ${t.tablet_fps}`;
    $("t-rtt").textContent = `${t.rtt_ms} ms`;
    $("t-mbps").textContent = `${t.mbps.toFixed(1)} Mbps`;
    $("t-enc").textContent = `codificação ${t.encode_ms.toFixed(1)} ms · ${t.encoder_kind === "GPU" ? "placa de vídeo" : "processador"}`;
  }

  $("wifi-desc").textContent = `No tablet, toque em “${s.name}” (${s.ip}).`;
  $("usb-desc").textContent = s.usb || "Conecte o cabo: com depuração USB liga na hora, ou compartilhe a internet do tablet pelo USB e toque no nome deste PC.";
  $("driver-desc").textContent = DRIVER[s.driver];
}

$("pair-close").onclick = () => {
  dismissedCode = $("pair-code").textContent;
  $("pair-dialog").close();
};

// ---- devices ----
async function loadPaired() {
  const list = await invoke("paired_devices");
  const box = $("paired");
  box.replaceChildren();
  if (!list.length) {
    box.innerHTML = '<div class="list-empty">Nenhum tablet pareado pelo Wi‑Fi ainda.</div>';
    return;
  }
  for (const [id, name] of list) {
    const row = document.createElement("div");
    row.className = "row";
    row.innerHTML = '<svg class="icon"><use href="#i-phone" /></svg><div class="label"><span class="title"></span><span class="desc">Pareado pelo Wi‑Fi</span></div><button class="btn">Esquecer</button>';
    row.querySelector(".title").textContent = name;
    row.querySelector("button").onclick = async () => {
      await invoke("forget_device", { id });
      loadPaired();
    };
    box.append(row);
  }
}

// ---- actions ----
$("install").onclick = async () => {
  $("install").disabled = true;
  $("install-msg").textContent = "Instalando o app no tablet…";
  $("install-msg").textContent = await invoke("install_apk");
  $("install").disabled = false;
};
$("restart").onclick = async () => {
  $("restart").disabled = true;
  $("driver-desc").textContent = "Reiniciando…";
  $("driver-desc").textContent = await invoke("restart_driver");
  $("restart").disabled = false;
};
$("autostart").onchange = async (e) => {
  const on = await invoke("set_autostart", { on: e.target.checked });
  e.target.checked = on;
  $("autostart-state").textContent = on ? "Ligado" : "Desligado";
};

async function load() {
  const info = await invoke("ui_info");
  document.body.classList.toggle("mica", info.mica);
  $("version").textContent = `Versão ${info.version}`;
  $("autostart").checked = info.autostart;
  $("autostart-state").textContent = info.autostart ? "Ligado" : "Desligado";

  settings = await invoke("get_settings");
  const [monitors, presets] = await invoke("options");
  $("resolution").replaceChildren(
    option("native", "A do tablet"),
    ...presets.flatMap(([w, h]) => [option(`${w}x${h}`, `${w}×${h}`), option(`${h}x${w}`, `${h}×${w} (retrato)`)]),
  );
  $("mirror-monitor").replaceChildren(
    option("", "Principal"),
    ...monitors.map(([name, w, h], i) => option(name, `Monitor ${i + 1} (${w}×${h})`)),
  );
  render();
  refresh();
  setInterval(refresh, 1000);
}

load();
