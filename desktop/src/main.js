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
let license = { valid: false };
let dismissedCode = null;
let lastStatus = null;
let receivedText = "";
let layoutDocument = { schema: 1, presets: [] };
let layoutPlanReady = null;

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
  $("audio-state").textContent = settings.audio ? "Ligado" : "Desligado";
  $("touch-state").textContent = settings.touch ? "Ligado" : "Desligado";
}

async function save(patch) {
  if (patch.mode === "extend" && !license.valid) {
    $("license-dialog").showModal(); // Estender needs a licence
    return;
  }
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
  lastStatus = s;
  const connected = Boolean(s.session);
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
      if (s.sessions.length > 1) {
        $("hero-detail").textContent = s.sessions.map((session) => `${session.name}: ${session.profile}`).join(" · ");
      }
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
    $("t-fps-s").textContent = `alvo ${t.target_fps} · tablet decodifica ${t.decode_fps} · renderiza ${t.tablet_fps}`;
    $("t-rtt").textContent = `${t.rtt_ms} ms`;
    $("t-mbps").textContent = `${t.mbps.toFixed(1)} Mbps`;
    const state = t.idle ? "ocioso" : `únicos ${t.unique_fps.toFixed(1)} · FLUSH ${t.repeated_fps.toFixed(1)}`;
    $("t-enc").textContent = `${t.codec} · ${t.encoder_kind === "GPU" ? "placa de vídeo" : "processador"} · encode p95 ${t.encode_p95_ms.toFixed(1)} ms · ${state}`;
  }

  $("wifi-desc").textContent = `No tablet, toque em “${s.name}” (${s.ip}).`;
  $("usb-desc").textContent = s.usb || "Conecte o cabo: com depuração USB liga na hora, ou compartilhe a internet do tablet pelo USB e toque no nome deste PC.";
  const adb = s.adb ?? [];
  const adbSelect = $("adb-device");
  const previous = adbSelect.value;
  adbSelect.replaceChildren(...adb.map(d => option(d.serial, `${d.model ?? d.serial} · ${d.state} · ${d.transport}`)));
  if (adb.some(d => d.serial === previous)) adbSelect.value = previous;
  $("install").disabled = adb.length === 0 || adb.every(d => d.state !== "device");
  $("driver-desc").textContent = DRIVER[s.driver];
  const textSession = (s.sessions ?? []).find(session => session.text_transfer);
  $("send-clipboard").disabled = !textSession;
  if (textSession && !$("clipboard-msg").textContent) $("clipboard-msg").textContent = "";
  const layoutSelect = $("layout-session");
  const previousLayoutSession = layoutSelect.value;
  layoutSelect.replaceChildren(...(s.sessions ?? []).map(session => option(String(session.id), session.name)));
  if ([...layoutSelect.options].some(item => item.value === previousLayoutSession)) layoutSelect.value = previousLayoutSession;
  $("save-layout").disabled = !layoutSelect.value;
  $("plan-layout").disabled = !layoutSelect.value;
  if (!layoutPlanReady || layoutPlanReady.sessionId !== layoutSelect.value) $("apply-layout").disabled = true;
  const recording = s.recording ?? { active: false, status: "Idle", error: null };
  const recordingSession = (s.sessions ?? [])[0];
  $("record-start").disabled = !recordingSession || recording.active;
  $("record-stop").disabled = !recording.active;
  $("recording-msg").textContent = recording.error
    ? `Gravação: ${recording.error}`
    : recording.active
      ? `Gravação: ${recording.status} · segmento ${recording.next_segment ?? 0}`
      : "Gravação inativa.";
  const incoming = await invoke("take_clipboard_text");
  if (incoming && !$("text-dialog").open) {
    receivedText = incoming.text;
    $("text-sender").textContent = incoming.sender;
    $("received-text").textContent = incoming.text;
    $("text-dialog").showModal();
  }
}

$("pair-close").onclick = () => {
  dismissedCode = $("pair-code").textContent;
  $("pair-dialog").close();
};

// ---- devices ----
async function loadPaired() {
  const list = await invoke("paired_devices");
  const box = $("paired");
  $("paired-msg").textContent = "";
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
      try {
        await invoke("forget_device", { id });
        await loadPaired();
      } catch (e) {
        $("paired-msg").textContent = `Não foi possível esquecer o tablet: ${e}`;
      }
    };
    box.append(row);
  }
}

// ---- actions ----
$("install").onclick = async () => {
  $("install").disabled = true;
  $("install-msg").textContent = "Instalando o app no tablet…";
  $("install-msg").textContent = await invoke("install_apk", { serial: $("adb-device").value || null });
  $("install").disabled = false;
};
$("restart").onclick = async () => {
  $("restart").disabled = true;
  $("driver-desc").textContent = "Reiniciando…";
  $("driver-desc").textContent = await invoke("restart_driver");
  $("restart").disabled = false;
};
$("export-metrics").onclick = async () => {
  $("export-metrics").disabled = true;
  try {
    $("metrics-msg").textContent = `Salvo em ${await invoke("export_metrics")}`;
  } catch (e) {
    $("metrics-msg").textContent = `Não consegui exportar: ${e}`;
  } finally {
    $("export-metrics").disabled = false;
  }
};
$("send-clipboard").onclick = async () => {
  const session = (lastStatus?.sessions ?? []).find(item => item.text_transfer);
  if (!session) return;
  try {
    if (!navigator.clipboard?.readText) throw new Error("A área de transferência não está disponível nesta janela.");
    const text = await navigator.clipboard.readText();
    await invoke("send_clipboard_text", { sessionId: session.id, text });
    $("clipboard-msg").textContent = "Texto enviado ao tablet.";
  } catch (e) {
    $("clipboard-msg").textContent = `Não consegui enviar: ${e}`;
  }
};
$("text-discard").onclick = () => $("text-dialog").close();
$("text-copy").onclick = async () => {
  try {
    await navigator.clipboard.writeText(receivedText);
    $("text-dialog").close();
  } catch (e) {
    $("clipboard-msg").textContent = `Não consegui copiar: ${e}`;
  }
};
$("save-layout").onclick = async () => {
  if (!$("layout-session").value) return;
  try {
    layoutDocument = await invoke("save_layout_preset", { sessionId: Number($("layout-session").value) });
    $("layout-msg").textContent = `Layout salvo para ${layoutDocument.presets.length} tablet(s).`;
  } catch (e) {
    $("layout-msg").textContent = `Não consegui salvar o layout: ${e}`;
  }
};
$("plan-layout").onclick = async () => {
  if (!$("layout-session").value) return;
  try {
    const plan = await invoke("plan_layout_preset", { sessionId: Number($("layout-session").value) });
    const adjustments = (plan.adjustments ?? []).map(item => `${item.field}: ${item.reason}`).join("; ");
    layoutPlanReady = !plan.requires_choice && !(plan.adjustments ?? []).length
      ? { sessionId: $("layout-session").value }
      : null;
    $("apply-layout").disabled = !layoutPlanReady;
    $("layout-msg").textContent = plan.requires_choice
      ? `Escolha necessária antes de aplicar: ${adjustments}`
      : adjustments ? `Ajuste explícito necessário: ${adjustments}` : "Preset pronto para confirmação; nenhuma alteração foi aplicada.";
  } catch (e) {
    $("layout-msg").textContent = `Não consegui verificar o layout: ${e}`;
  }
};
$("apply-layout").onclick = async () => {
  if (!layoutPlanReady) return;
  try {
    await invoke("apply_layout_preset", { sessionId: Number(layoutPlanReady.sessionId) });
    layoutPlanReady = null;
    $("apply-layout").disabled = true;
    $("layout-msg").textContent = "Preset aplicado ao único tablet conectado.";
  } catch (e) {
    $("layout-msg").textContent = `Não consegui aplicar o layout: ${e}`;
  }
};
$("record-start").onclick = async () => {
  const session = (lastStatus?.sessions ?? [])[0];
  const path = $("recording-path").value.trim();
  if (!session || !path) {
    $("recording-msg").textContent = "Escolha um tablet conectado e um destino .mkv.";
    return;
  }
  $("record-start").disabled = true;
  try {
    await invoke("start_recording", { sessionId: session.id, path });
    $("recording-msg").textContent = "Gravação iniciada; o primeiro arquivo aguarda configuração e IDR.";
  } catch (e) {
    $("recording-msg").textContent = `Não consegui iniciar a gravação: ${e}`;
  }
};
$("record-stop").onclick = async () => {
  $("record-stop").disabled = true;
  try {
    const paths = await invoke("stop_recording");
    $("recording-msg").textContent = paths.length
      ? `Gravação finalizada: ${paths.join("; ")}`
      : "Gravação finalizada sem um segmento decodificável.";
  } catch (e) {
    $("recording-msg").textContent = `Não consegui finalizar a gravação: ${e}`;
  }
};
$("autostart").onchange = async (e) => {
  const on = await invoke("set_autostart", { on: e.target.checked });
  e.target.checked = on;
  $("autostart-state").textContent = on ? "Ligado" : "Desligado";
};

// ---- updates ----
async function checkUpdate(silent) {
  if (!silent) $("update-desc").textContent = "Procurando…";
  try {
    const update = await invoke("check_update");
    $("update-banner").hidden = !update;
    if (update) {
      $("update-version").textContent = update.version;
      $("update-notes").textContent = update.notes ?? "";
      $("update-desc").textContent = `A versão ${update.version} está disponível (veja o aviso em Início).`;
    } else if (!silent) {
      $("update-desc").textContent = "Você já está na versão mais recente.";
    }
  } catch (e) {
    if (!silent) $("update-desc").textContent = `Não consegui verificar: ${e}`;
  }
}
$("update-check").onclick = () => checkUpdate(false);
$("update-install").onclick = async () => {
  $("update-install").disabled = true;
  $("update-install").textContent = "Baixando…";
  try {
    await invoke("install_update"); // the app restarts when it's done
  } catch (e) {
    $("update-install").disabled = false;
    $("update-install").textContent = "Instalar e reiniciar";
    $("update-desc").textContent = `A atualização falhou: ${e}`;
  }
};

// ---- licence ----
function renderLicense() {
  $("license-desc").textContent = license.valid ? `Ativa para ${license.name} (${license.email}). Estender liberado.` : "Sem licença: só Espelhar. Estender pede uma licença.";
  $("license-activate").textContent = license.valid ? "Remover" : "Ativar";
  $("license-token").hidden = license.valid;
  for (const b of $$('.seg[data-setting="mode"] button[data-value="extend"]')) b.title = license.valid ? "" : "Requer licença";
}
$("license-activate").onclick = async () => {
  if (license.valid) {
    license = await invoke("remove_license");
    settings = await invoke("get_settings");
    render();
  } else {
    try {
      license = await invoke("activate_license", { token: $("license-token").value });
      $("license-token").value = "";
    } catch (e) {
      $("license-desc").textContent = String(e);
      return;
    }
  }
  renderLicense();
};
$("license-buy").onclick = () => invoke("open_buy_page");
$("license-close").onclick = () => $("license-dialog").close();

// ---- first-run guide ----
const GUIDE_TABLET = `<p>Instale o app no tablet de um destes jeitos:</p><ol>
  <li><b>USB com depuração:</b> ligue a depuração USB nas opções do desenvolvedor, conecte o cabo e use “Instalar app no tablet” na página Início.</li>
  <li><b>USB sem depuração:</b> conecte o cabo, ligue o compartilhamento de internet por USB no tablet e toque no nome deste PC assim que aparecer.</li>
  <li><b>Manual:</b> instale o APK do TabDisplay no tablet e conecte pelo Wi‑Fi (mesma rede do PC).</li></ol>`;
const GUIDE_PAIR = `<p>Abra o TabDisplay no tablet e toque no nome deste PC.</p>
  <ul><li><b>Wi‑Fi:</b> na primeira vez, o PC mostra um código de 6 dígitos; digite no tablet. Depois não é pedido mais.</li>
  <li><b>ADB local ou rede USB:</b> a primeira conexão também pede o código; depois o pareamento fica guardado para este PC.</li></ul>`;

async function guideSteps() {
  const info = await invoke("ui_info");
  const s = await invoke("status");
  const mac = info.os === "macos";
  const first = mac
    ? {
        title: "Permissões do macOS",
        body: `<p>O TabDisplay pede três permissões:</p><ul>
        <li><b>Gravação de Tela:</b> para enviar a imagem do Mac ao tablet.</li>
        <li><b>Acessibilidade:</b> para o toque e a caneta do tablet controlarem o Mac.</li>
        <li><b>Rede Local:</b> para o tablet achar este Mac e conectar.</li></ul><p class="muted"></p>`,
        driver: DRIVER[s.driver] ?? "",
      }
    : {
        title: "Monitor virtual",
        body: `<p>O monitor virtual é o que faz o tablet virar uma segunda tela.</p><p class="muted"></p>${
          s.driver === "ok" ? "" : "<p>Se algo falhou, use “Reiniciar” em Avançado ou reinstale o app.</p>"
        }`,
        driver: DRIVER[s.driver] ?? "",
      };
  return [first, { title: "Instalar o app no tablet", body: GUIDE_TABLET }, { title: "Conectar e parear", body: GUIDE_PAIR }, { title: "Tudo pronto", body: "<p>Depois de conectado, escolha em <b>Início</b> se o tablet estende a tela ou espelha, e a qualidade em <b>Tela</b>.</p><p class=\"muted\">Você pode rever este guia em Avançado.</p>" }];
}

async function showGuide() {
  const steps = await guideSteps();
  let i = 0;
  const draw = () => {
    $("guide-title").textContent = steps[i].title;
    const template = document.createElement("template");
    // These are repository-owned static templates; dynamic diagnostics are
    // inserted below with textContent, never interpolated into HTML.
    template.innerHTML = steps[i].body;
    $("guide-body").replaceChildren(template.content.cloneNode(true));
    const driver = $("guide-body").querySelector(".muted");
    if (driver) driver.textContent = steps[i].driver ?? "";
    $("guide-step").textContent = `${i + 1} de ${steps.length}`;
    $("guide-back").hidden = i === 0;
    $("guide-next").textContent = i === steps.length - 1 ? "Concluir" : "Próximo";
  };
  const close = () => {
    $("guide").close();
    if (!settings.onboarded) save({ onboarded: true });
  };
  $("guide-skip").onclick = close;
  $("guide-back").onclick = () => { i--; draw(); };
  $("guide-next").onclick = () => (i === steps.length - 1 ? close() : (i++, draw()));
  $("guide").oncancel = () => { if (!settings.onboarded) save({ onboarded: true }); }; // Esc counts as skipping
  draw();
  $("guide").showModal();
}
$("show-guide").onclick = showGuide;

async function load() {
  const info = await invoke("ui_info");
  document.body.classList.toggle("mica", info.mica);
  $("version").textContent = `Versão ${info.version}`;
  $("autostart").checked = info.autostart;
  $("autostart-state").textContent = info.autostart ? "Ligado" : "Desligado";

  license = await invoke("license_status");
  renderLicense();
  settings = await invoke("get_settings");
  try {
    layoutDocument = await invoke("layout_presets");
  } catch (e) {
    $("layout-msg").textContent = `Não consegui ler os layouts salvos: ${e}`;
  }
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
  if (!settings.onboarded) showGuide();
  setTimeout(() => checkUpdate(true), 8000); // quietly, a few seconds after opening
}

load();
