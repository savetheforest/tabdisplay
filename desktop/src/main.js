const { invoke } = window.__TAURI__.core;
const $ = (id) => document.getElementById(id);

async function refresh() {
  const [ip, status] = await invoke("status");
  $("ip").textContent = ip;
  $("status").textContent = status;
}

$("usb").onclick = async () => ($("msg").textContent = await invoke("adb_reverse"));
refresh();
setInterval(refresh, 1000);
