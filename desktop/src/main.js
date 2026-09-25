const { invoke } = window.__TAURI__.core;
const $ = (id) => document.getElementById(id);

const DRIVER = {
  ok: "Driver ativo.",
  parado: "Driver parado: clique em Reiniciar driver (pede admin).",
  ausente: "Driver não instalado: só dá para espelhar.",
};

let settings;

async function refresh() {
  const [ip, status, driver] = await invoke("status");
  $("ip").textContent = ip;
  $("status").textContent = status;
  $("driver").textContent = DRIVER[driver];
}

function option(value, text) {
  const o = document.createElement("option");
  o.value = value;
  o.textContent = text;
  return o;
}

async function load() {
  settings = await invoke("get_settings");
  const [monitors, presets] = await invoke("options");
  $("resolution").replaceChildren(
    option("native", "Nativa do tablet"),
    ...presets.flatMap(([w, h]) => [option(`${w}x${h}`, `${w}×${h}`), option(`${h}x${w}`, `${h}×${w} (retrato)`)]),
  );
  $("mirror_monitor").replaceChildren(
    option("", "Principal"),
    ...monitors.map(([name, w, h], i) => option(name, `Monitor ${i + 1} (${w}×${h})`)),
  );

  $("mode").value = settings.mode;
  $("resolution").value = settings.resolution ? settings.resolution.join("x") : "native";
  $("position").value = settings.position;
  $("mirror_monitor").value = settings.mirror_monitor ?? "";
  $("fps").value = settings.fps;
  $("bitrate_mbps").value = settings.bitrate_mbps;
  $("encoder").value = settings.encoder;
  $("touch").checked = settings.touch;
  render();
}

function render() {
  for (const el of document.querySelectorAll("[data-for]")) el.hidden = el.dataset.for !== $("mode").value;
  $("bitrate_label").textContent = `${$("bitrate_mbps").value} Mbps`;
}

async function save() {
  const res = $("resolution").value;
  settings = {
    mode: $("mode").value,
    resolution: res === "native" ? null : res.split("x").map(Number),
    position: $("position").value,
    mirror_monitor: $("mirror_monitor").value || null,
    fps: Number($("fps").value),
    bitrate_mbps: Number($("bitrate_mbps").value),
    encoder: $("encoder").value,
    touch: $("touch").checked,
  };
  render();
  await invoke("set_settings", { settings });
}

$("form").addEventListener("change", save);
$("bitrate_mbps").addEventListener("input", render);
$("usb").onclick = async () => ($("msg").textContent = await invoke("adb_reverse"));
$("restart").onclick = async () => {
  $("restart").disabled = true;
  $("driver").textContent = "Reiniciando…";
  $("driver").textContent = await invoke("restart_driver");
  $("restart").disabled = false;
};

load();
refresh();
setInterval(refresh, 1000);
