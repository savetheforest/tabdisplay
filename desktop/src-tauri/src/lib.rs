mod audio;
mod avcc;
mod encode;
// The file-transfer core is intentionally not wired to the product surface yet.
#[allow(dead_code)]
mod files;
// Versioned, bounded wire contract; channel/receptor/SAF integration remains pending.
#[allow(dead_code)]
mod file_protocol;
mod input;
// Preset schema and presentation planning remain intentionally unadvertised.
#[allow(dead_code)]
mod layouts;
// Recording remains unadvertised until its explicit writer bridge is integrated
// with the session lifecycle and user-facing controls.
mod license;
#[allow(dead_code)]
mod matroska;
mod pairing;
#[allow(dead_code)]
mod recording;
mod server;
mod settings;
mod telemetry;
mod tls;
mod update;
#[cfg(test)]
pub(crate) mod test_fixtures {
    pub fn hex(name: &str) -> Vec<u8> {
        let source = match name {
            "input-touch-pen.hex" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test-fixtures/v3/input-touch-pen.hex"
            )),
            "scroll.hex" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test-fixtures/v3/scroll.hex"
            )),
            _ => panic!("unknown fixture: {name}"),
        };
        source
            .split_whitespace()
            .map(|byte| u8::from_str_radix(byte, 16).unwrap())
            .collect()
    }

    pub fn text(name: &str) -> &'static str {
        match name {
            "hello.json" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test-fixtures/v3/hello.json"
            )),
            "config.json" => include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../test-fixtures/v3/config.json"
            )),
            _ => panic!("unknown fixture: {name}"),
        }
    }
}
// Platform layer: same module names and APIs on each OS; the rest of the app uses `sys::…`.
#[cfg(windows)]
mod win {
    pub mod audio;
    pub mod capture;
    pub mod display;
    pub mod driver;
    pub mod encode;
    pub mod input;
    pub mod service;
}
#[cfg(target_os = "macos")]
mod mac {
    pub mod audio;
    pub mod capture;
    pub mod display;
    pub mod encode;
    pub mod input;
    pub mod permissions;
}
#[cfg(target_os = "macos")]
use mac as sys;
#[cfg(windows)]
use win as sys;

use serde::Serialize;
use settings::Settings;
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sys::{capture, display};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::{Effect, EffectsBuilder};
use tauri::Manager;
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

#[cfg(windows)]
pub use win::driver::cli as driver_cli;

/// Last result of the automatic `adb reverse`, shown in the UI.
static USB: Mutex<&str> = Mutex::new("");
static ADB_DEVICES: Mutex<Vec<AdbDevice>> = Mutex::new(Vec::new());

#[derive(Clone, Serialize)]
struct AdbDevice {
    serial: String,
    state: String,
    model: Option<String>,
    transport: String,
}

/// Everything the UI polls once a second.
#[tauri::command]
fn status() -> serde_json::Value {
    let addresses = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|iface| match iface.addr {
            if_addrs::IfAddr::V4(v4) if !v4.ip.is_loopback() => Some(v4.ip.to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let ip = if addresses.is_empty() {
        "?".into()
    } else {
        addresses.join(", ")
    };
    let sessions = server::sessions();
    let pairing = pairing::current()
        .map(|(code, device)| serde_json::json!({ "code": code, "device": device }));
    serde_json::json!({
        "ip": ip,
        "name": server::computer_name(),
        "status": sessions.first().map_or_else(|| server::STATUS.lock().unwrap().clone(), |s| s.status.clone()),
        "driver": display::driver_state(),
        "usb": *USB.lock().unwrap(),
        "adb": ADB_DEVICES.lock().unwrap().clone(),
        "pairing": pairing,
        "profile": settings::get().profile,
        "stats": sessions.first().and_then(|s| s.stats.clone()),
        "session": (!sessions.is_empty()).then(|| sessions.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(" + ")),
        "sessions": sessions,
        "recording": server::recording_status(),
    })
}

#[tauri::command]
fn recording_status() -> server::RecordingInfo {
    server::recording_status()
}

/// Starts recording only after an explicit desktop action and a selected path.
#[tauri::command]
fn start_recording(session_id: u64, path: String) -> Result<(), String> {
    server::start_recording(session_id, path)
}

#[tauri::command]
fn stop_recording() -> Result<Vec<String>, String> {
    server::stop_recording()
}

/// Receives one explicit tablet text action; it is consumed from process memory.
#[tauri::command]
fn take_clipboard_text() -> Option<server::ReceivedText> {
    server::take_text()
}

/// Sends text after an explicit user action in the desktop UI.
#[tauri::command]
fn send_clipboard_text(session_id: u64, text: String) -> Result<(), String> {
    server::send_text(session_id, &text)
}

/// Explicitly exports a sanitized metrics snapshot to the user's Downloads folder.
#[tauri::command]
fn export_metrics(app: tauri::AppHandle) -> Result<String, String> {
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let path = dir.join(format!("tabdisplay-metricas-{stamp}.json"));
    let content =
        serde_json::to_vec_pretty(&server::metrics_report()).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn paired_devices() -> Vec<(String, String)> {
    pairing::devices()
        .into_iter()
        .map(|d| (d.id, d.name))
        .collect()
}

#[tauri::command]
fn forget_device(id: String) -> Result<(), String> {
    pairing::forget(&id)?;
    server::disconnect_device(&id);
    Ok(())
}

#[tauri::command]
fn get_settings() -> Settings {
    settings::get()
}

#[tauri::command]
fn set_settings(settings: Settings) -> Result<(), String> {
    settings::set(settings)?;
    sync_tray(settings::get().mode);
    Ok(())
}

/// Choices for the UI: (attached monitors as (name, w, h), extend resolution presets).
#[tauri::command]
fn options() -> (Vec<(String, u32, u32)>, &'static [(u32, u32)]) {
    let virtual_monitors = display::find_devices();
    let monitors = capture::monitors()
        .into_iter()
        .filter(|m| !virtual_monitors.contains(&m.0))
        .collect();
    (monitors, display::PRESETS)
}

fn layouts_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("layouts.json"))
        .map_err(|error| error.to_string())
}

fn layout_error(error: layouts::PresetError) -> String {
    format!("Falha nos layouts: {error:?}")
}

#[tauri::command]
fn layout_presets(app: tauri::AppHandle) -> Result<layouts::PresetDocument, String> {
    layouts::load(&layouts_path(&app)?).map_err(layout_error)
}

#[tauri::command]
fn save_layout_preset(
    app: tauri::AppHandle,
    session_id: u64,
) -> Result<layouts::PresetDocument, String> {
    let session = server::sessions()
        .into_iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| "Sessão do tablet não encontrada.".to_string())?;
    let mut document = layouts::load(&layouts_path(&app)?).map_err(layout_error)?;
    let preset = layouts::LayoutPreset::from_settings(&session.device_id, &settings::get())
        .map_err(layout_error)?;
    document.upsert(preset).map_err(layout_error)?;
    layouts::save(&layouts_path(&app)?, &document).map_err(layout_error)?;
    Ok(document)
}

/// Computes, but does not apply, the changes needed by a saved tablet preset.
/// A missing monitor or licence is returned to the UI as an explicit choice.
#[tauri::command]
fn plan_layout_preset(
    app: tauri::AppHandle,
    session_id: u64,
) -> Result<layouts::ApplyPlan, String> {
    let session = server::sessions()
        .into_iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| "Sessão do tablet não encontrada.".to_string())?;
    let document = layouts::load(&layouts_path(&app)?).map_err(layout_error)?;
    let preset = document
        .presets
        .into_iter()
        .find(|preset| preset.device_id == session.device_id)
        .ok_or_else(|| "Nenhum layout salvo para este tablet.".to_string())?;
    let monitors = capture::monitors()
        .into_iter()
        .map(|monitor| monitor.0)
        .collect::<Vec<_>>();
    Ok(layouts::plan_apply(preset, license::valid(), &monitors))
}

/// Applies a saved preset only with one active session and no unresolved
/// adjustment. Multi-tablet application remains deliberately explicit until
/// each session has its own complete settings object.
#[tauri::command]
fn apply_layout_preset(app: tauri::AppHandle, session_id: u64) -> Result<(), String> {
    let sessions = server::sessions();
    if sessions.len() != 1 || sessions[0].id != session_id {
        return Err("Aplicação automática exige exatamente um tablet conectado; verifique o plano primeiro.".into());
    }
    let session = sessions.into_iter().next().unwrap();
    let document = layouts::load(&layouts_path(&app)?).map_err(layout_error)?;
    let preset = document
        .presets
        .into_iter()
        .find(|preset| preset.device_id == session.device_id)
        .ok_or_else(|| "Nenhum layout salvo para este tablet.".to_string())?;
    let monitors = capture::monitors()
        .into_iter()
        .map(|monitor| monitor.0)
        .collect::<Vec<_>>();
    let plan = layouts::plan_apply(preset.clone(), license::valid(), &monitors);
    if plan.requires_choice || !plan.adjustments.is_empty() {
        return Err("O preset precisa de uma escolha explícita antes de ser aplicado.".into());
    }
    settings::set(preset.apply_to(settings::get()))
}

#[tauri::command]
async fn restart_driver() -> String {
    #[cfg(not(windows))]
    return "Não é necessário neste sistema.".into();
    #[cfg(windows)]
    match win::service::restart() {
        Ok(()) => "Monitor virtual reiniciado.".into(),
        Err(e) => {
            telemetry::warn(format!("driver restart failed: {e}"));
            format!("Falhou: {e}")
        }
    }
}

/// adb from PATH if there is one, so we don't fight a different adb version (e.g. Android Studio's)
/// over the shared adb server; otherwise the bundled copy.
fn adb(args: &[&str]) -> std::io::Result<Output> {
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    let path = PATH.get_or_init(|| {
        let on_path = hidden(Command::new("adb").arg("version")).output().is_ok();
        if on_path {
            "adb".into()
        } else {
            resource(if cfg!(windows) {
                "adb/adb.exe"
            } else {
                "adb/adb"
            })
        }
    });
    let mut child = hidden(
        Command::new(path)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
    .spawn()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if child.try_wait()?.is_some() {
            return child.wait_with_output();
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "adb timed out",
            ));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn adb_devices() -> Vec<AdbDevice> {
    let Ok(output) = adb(&["devices", "-l"]) else {
        return Vec::new();
    };
    parse_adb_devices(&String::from_utf8_lossy(&output.stdout))
}

fn parse_adb_devices(text: &str) -> Vec<AdbDevice> {
    text.lines()
        .skip(1)
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if fields.len() < 2 {
                return None;
            }
            let serial = fields[0].to_string();
            let state = fields[1].to_string();
            let model = fields
                .iter()
                .find_map(|field| field.strip_prefix("model:"))
                .map(|m| m.replace('_', " "));
            let transport = if fields.iter().any(|field| field.starts_with("usb:")) {
                "USB".to_string()
            } else if serial.contains(':') {
                "ADB network".to_string()
            } else {
                "ADB".to_string()
            };
            Some(AdbDevice {
                serial,
                state,
                model,
                transport,
            })
        })
        .collect()
}

/// No console window flashing up for each adb call on Windows.
fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(cmd, 0x0800_0000); // CREATE_NO_WINDOW
    cmd
}

/// A file bundled with the app (Windows: next to the exe; macOS: in the .app's Resources).
fn resource(name: &str) -> std::path::PathBuf {
    APP.get()
        .and_then(|a| a.path().resource_dir().ok())
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap_or_default()
                .with_file_name("")
        })
        .join(name)
}

/// Keeps `adb reverse` set up so a tablet on the cable can reach this PC at its 127.0.0.1.
// ponytail: polls adb every 3s; switch to `adb track-devices` if the process spawning ever matters.
fn usb_forward() {
    let port = format!("tcp:{}", server::PORT);
    loop {
        let devices = adb_devices();
        let mut usb_ready = 0;
        for device in &devices {
            if device.state == "device" && device.transport == "USB" {
                let args = [
                    "-s",
                    device.serial.as_str(),
                    "reverse",
                    port.as_str(),
                    port.as_str(),
                ];
                if adb(&args).is_ok_and(|o| o.status.success()) {
                    usb_ready += 1;
                }
            }
        }
        *ADB_DEVICES.lock().unwrap() = devices;
        *USB.lock().unwrap() = if usb_ready > 0 {
            "Tablet USB pronto: selecione o aparelho no instalador ou toque no PC no tablet."
        } else {
            ""
        };
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// Installs the bundled tablet app over the cable (USB debugging must be on).
#[tauri::command]
async fn install_apk(serial: Option<String>) -> String {
    let apk = resource("tabdisplay.apk");
    let devices = adb_devices();
    let selected = match serial.or_else(|| (devices.len() == 1).then(|| devices[0].serial.clone()))
    {
        Some(serial) => serial,
        None if devices.is_empty() => {
            return "Nenhum aparelho ADB disponível; autorize a depuração USB e conecte um tablet."
                .into()
        }
        None => return "Há mais de um aparelho ADB; selecione um tablet explicitamente.".into(),
    };
    let Some(device) = devices.iter().find(|device| device.serial == selected) else {
        return "O aparelho escolhido não está mais conectado; atualize a lista.".into();
    };
    if device.state != "device" {
        return format!(
            "O aparelho {} está {}, não autorizado para instalação.",
            device.serial, device.state
        );
    }
    let apk_path = apk.display().to_string();
    let args = ["-s", selected.as_str(), "install", "-r", apk_path.as_str()];
    match adb(&args) {
        Ok(o) if o.status.success() => "App instalado no tablet.".into(),
        Ok(o) => {
            let message = format!(
                "{} {}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            if message.contains("INSTALL_FAILED_UPDATE_INCOMPATIBLE") {
                "A instalação foi recusada: a assinatura do APK é diferente. Não desinstale automaticamente; remova o app antigo manualmente se quiser perder os dados.".into()
            } else {
                format!(
                    "Falhou: {}",
                    message.trim().chars().take(500).collect::<String>()
                )
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
            "O ADB demorou demais; verifique o aparelho e tente novamente.".into()
        }
        Err(e) => format!("adb indisponível ({e})"),
    }
}

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
/// The tray's Extend/Mirror checks, kept in sync with the settings.
static TRAY_MODE: OnceLock<(CheckMenuItem<tauri::Wry>, CheckMenuItem<tauri::Wry>)> =
    OnceLock::new();

/// Brings the window up: tray click, a second launch, or a tablet asking to pair (to show the code).
pub fn show_window() {
    if let Some(w) = APP.get().and_then(|a| a.get_webview_window("main")) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn sync_tray(mode: settings::Mode) {
    if let Some((extend, mirror)) = TRAY_MODE.get() {
        let _ = extend.set_checked(mode == settings::Mode::Extend);
        let _ = mirror.set_checked(mode == settings::Mode::Mirror);
    }
}

fn tray(app: &tauri::App) -> tauri::Result<()> {
    let mode = settings::get().mode;
    let open = MenuItem::with_id(app, "open", "Abrir TabDisplay", true, None::<&str>)?;
    let extend = CheckMenuItem::with_id(
        app,
        "extend",
        "Estender (2º monitor)",
        true,
        mode == settings::Mode::Extend,
        None::<&str>,
    )?;
    let mirror = CheckMenuItem::with_id(
        app,
        "mirror",
        "Espelhar",
        true,
        mode == settings::Mode::Mirror,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &open,
            &PredefinedMenuItem::separator(app)?,
            &extend,
            &mirror,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;
    let _ = TRAY_MODE.set((extend, mirror));
    TrayIconBuilder::with_id("tray")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("TabDisplay")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_window(),
            id @ ("extend" | "mirror") => {
                let mut s = settings::get();
                s.mode = if id == "extend" {
                    settings::Mode::Extend
                } else {
                    settings::Mode::Mirror
                };
                let _ = settings::set(s);
                sync_tray(settings::get().mode); // stays Mirror without a licence
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|_, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_window();
            }
        })
        .build(app)?;
    Ok(())
}

/// Mica needs Windows 11 (build 22000+); on Windows 10 a transparent window would just be see-through.
#[cfg(not(windows))]
fn supports_mica() -> bool {
    false
}
#[cfg(windows)]
fn supports_mica() -> bool {
    let mut v = windows::Win32::System::SystemInformation::OSVERSIONINFOW {
        dwOSVersionInfoSize: size_of::<windows::Win32::System::SystemInformation::OSVERSIONINFOW>()
            as u32,
        ..Default::default()
    };
    unsafe { windows::Wdk::System::SystemServices::RtlGetVersion(&mut v) }.is_ok()
        && v.dwBuildNumber >= 22000
}

/// Look-and-feel facts the UI needs once.
#[tauri::command]
fn ui_info(app: tauri::AppHandle) -> serde_json::Value {
    serde_json::json!({
        "mica": supports_mica(),
        "os": std::env::consts::OS,
        "version": app.package_info().version.to_string(),
        "autostart": app.autolaunch().is_enabled().unwrap_or(false),
    })
}

#[cfg(test)]
mod adb_tests {
    use super::*;

    #[test]
    fn parses_states_and_transport_without_shell_interpolation() {
        let devices = parse_adb_devices(
            "List of devices attached\nusb-1 device product:foo model:Redmi_Pad_2 device:jade usb:1-2 transport_id:4\n192.0.2.4:5555 unauthorized transport_id:7\noffline-serial offline\n",
        );
        assert_eq!(devices.len(), 3);
        assert_eq!(
            (
                devices[0].serial.as_str(),
                devices[0].state.as_str(),
                devices[0].transport.as_str()
            ),
            ("usb-1", "device", "USB")
        );
        assert_eq!(devices[0].model.as_deref(), Some("Redmi Pad 2"));
        assert_eq!(devices[1].state, "unauthorized");
        assert_eq!(devices[1].transport, "ADB network");
        assert_eq!(devices[2].state, "offline");
    }
}

#[tauri::command]
fn license_status() -> serde_json::Value {
    let l = license::current();
    serde_json::json!({ "valid": l.is_some(), "name": l.as_ref().map(|l| l.name.clone()), "email": l.map(|l| l.email), "buy_url": license::BUY_URL })
}

#[tauri::command]
fn activate_license(token: String) -> Result<serde_json::Value, String> {
    license::activate(&token).map_err(String::from)?;
    Ok(license_status())
}

#[tauri::command]
fn remove_license() -> serde_json::Value {
    license::remove();
    let _ = settings::set(settings::get()); // drops Extend now that it's no longer allowed
    sync_tray(settings::get().mode);
    license_status()
}

#[tauri::command]
fn open_buy_page() {
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", license::BUY_URL])
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open")
        .arg(license::BUY_URL)
        .spawn();
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, on: bool) -> bool {
    let launcher = app.autolaunch();
    let _ = if on {
        launcher.enable()
    } else {
        launcher.disable()
    };
    launcher.is_enabled().unwrap_or(false)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _sentry = telemetry::init(); // reports panics from any thread; off without a DSN
    tauri::Builder::default()
        // A second launch (Start menu, autostart) focuses the running window instead of fighting for port 7070.
        .plugin(tauri_plugin_single_instance::init(|_, _, _| show_window()))
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Started with Windows: come up in the tray, not in the user's face.
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            let _ = APP.set(app.handle().clone());
            license::init(app.path().app_config_dir()?); // before settings: extending needs it
            settings::init(app.path().app_config_dir()?);
            pairing::init(app.path().app_config_dir()?);
            tls::init(app.path().app_config_dir()?);
            tray(app)?;
            #[cfg(target_os = "macos")]
            mac::permissions::request();
            if let Some(window) = app.get_webview_window("main") {
                if supports_mica() {
                    let _ = window.set_effects(EffectsBuilder::new().effect(Effect::Mica).build());
                }
                if std::env::args().any(|a| a == "--minimized") {
                    let _ = window.hide();
                }
            }
            std::thread::spawn(server::run);
            std::thread::spawn(server::beacon);
            std::thread::spawn(usb_forward);
            Ok(())
        })
        // Closing the window keeps TabDisplay running in the tray; "Sair" in the tray quits.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            status,
            recording_status,
            start_recording,
            stop_recording,
            take_clipboard_text,
            send_clipboard_text,
            export_metrics,
            update::check_update,
            update::install_update,
            license_status,
            activate_license,
            remove_license,
            open_buy_page,
            get_settings,
            set_settings,
            options,
            layout_presets,
            save_layout_preset,
            plan_layout_preset,
            apply_layout_preset,
            restart_driver,
            install_apk,
            paired_devices,
            forget_device,
            ui_info,
            set_autostart
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
