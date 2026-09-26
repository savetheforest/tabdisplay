mod audio;
mod encode;
mod input;
mod pairing;
mod server;
mod settings;
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
#[cfg(windows)]
use win as sys;
#[cfg(target_os = "macos")]
use mac as sys;

use settings::Settings;
use std::net::UdpSocket;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::window::{Effect, EffectsBuilder};
use tauri::Manager;
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use sys::{capture, display};

#[cfg(windows)]
pub use win::driver::cli as driver_cli;

/// Last result of the automatic `adb reverse`, shown in the UI.
static USB: Mutex<&str> = Mutex::new("");

/// Everything the UI polls once a second.
#[tauri::command]
fn status() -> serde_json::Value {
    // Connecting a UDP socket sends nothing; it just makes the OS pick the outbound interface.
    let ip = UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("8.8.8.8:80").and(s.local_addr()))
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "?".into());
    let pairing = pairing::current().map(|(code, device)| serde_json::json!({ "code": code, "device": device }));
    serde_json::json!({
        "ip": ip,
        "name": server::computer_name(),
        "status": server::STATUS.lock().unwrap().clone(),
        "driver": display::driver_state(),
        "usb": *USB.lock().unwrap(),
        "pairing": pairing,
        "profile": settings::get().profile,
        "stats": server::STATS.lock().unwrap().clone(),
        "session": server::SESSION.lock().unwrap().clone(),
    })
}

#[tauri::command]
fn paired_devices() -> Vec<(String, String)> {
    pairing::devices().into_iter().map(|d| (d.id, d.name)).collect()
}

#[tauri::command]
fn forget_device(id: String) {
    pairing::forget(&id);
}

#[tauri::command]
fn get_settings() -> Settings {
    settings::get()
}

#[tauri::command]
fn set_settings(settings: Settings) {
    sync_tray(settings.mode);
    settings::set(settings);
}

/// Choices for the UI: (attached monitors as (name, w, h), extend resolution presets).
#[tauri::command]
fn options() -> (Vec<(String, u32, u32)>, &'static [(u32, u32)]) {
    let virtual_monitor = display::find_device();
    let monitors = capture::monitors().into_iter().filter(|m| Some(&m.0) != virtual_monitor.as_ref()).collect();
    (monitors, display::PRESETS)
}

#[tauri::command]
async fn restart_driver() -> String {
    #[cfg(not(windows))]
    return "Não é necessário neste sistema.".into();
    #[cfg(windows)]
    match win::service::restart() {
        Ok(()) => "Monitor virtual reiniciado.".into(),
        Err(e) => format!("Falhou: {e}"),
    }
}

/// adb from PATH if there is one, so we don't fight a different adb version (e.g. Android Studio's)
/// over the shared adb server; otherwise the bundled copy.
fn adb(args: &[&str]) -> std::io::Result<std::process::Output> {
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    let path = PATH.get_or_init(|| {
        let on_path = hidden(Command::new("adb").arg("version")).output().is_ok();
        if on_path { "adb".into() } else { resource(if cfg!(windows) { "adb/adb.exe" } else { "adb/adb" }) }
    });
    hidden(Command::new(path).args(args)).output()
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
        .unwrap_or_else(|| std::env::current_exe().unwrap_or_default().with_file_name(""))
        .join(name)
}

/// Keeps `adb reverse` set up so a tablet on the cable can reach this PC at its 127.0.0.1.
// ponytail: polls adb every 3s; switch to `adb track-devices` if the process spawning ever matters.
fn usb_forward() {
    let port = format!("tcp:{}", server::PORT);
    loop {
        let ok = adb(&["reverse", &port, &port]).is_ok_and(|o| o.status.success());
        *USB.lock().unwrap() = if ok { "Tablet conectado pelo cabo: toque em \"PC pelo cabo USB\" no tablet." } else { "" };
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// Installs the bundled tablet app over the cable (USB debugging must be on).
#[tauri::command]
async fn install_apk() -> String {
    let apk = resource("tabdisplay.apk");
    match adb(&["install", "-r", &apk.display().to_string()]) {
        Ok(o) if o.status.success() => "App instalado no tablet.".into(),
        Ok(o) => format!("Falhou: {}", String::from_utf8_lossy(&o.stdout).trim()),
        Err(e) => format!("adb indisponível ({e})"),
    }
}

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();
/// The tray's Extend/Mirror checks, kept in sync with the settings.
static TRAY_MODE: OnceLock<(CheckMenuItem<tauri::Wry>, CheckMenuItem<tauri::Wry>)> = OnceLock::new();

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
    let extend = CheckMenuItem::with_id(app, "extend", "Estender (2º monitor)", true, mode == settings::Mode::Extend, None::<&str>)?;
    let mirror = CheckMenuItem::with_id(app, "mirror", "Espelhar", true, mode == settings::Mode::Mirror, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &PredefinedMenuItem::separator(app)?, &extend, &mirror, &PredefinedMenuItem::separator(app)?, &quit])?;
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
                s.mode = if id == "extend" { settings::Mode::Extend } else { settings::Mode::Mirror };
                sync_tray(s.mode);
                settings::set(s);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|_, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
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
        dwOSVersionInfoSize: size_of::<windows::Win32::System::SystemInformation::OSVERSIONINFOW>() as u32,
        ..Default::default()
    };
    unsafe { windows::Wdk::System::SystemServices::RtlGetVersion(&mut v) }.is_ok() && v.dwBuildNumber >= 22000
}

/// Look-and-feel facts the UI needs once.
#[tauri::command]
fn ui_info(app: tauri::AppHandle) -> serde_json::Value {
    serde_json::json!({
        "mica": supports_mica(),
        "version": app.package_info().version.to_string(),
        "autostart": app.autolaunch().is_enabled().unwrap_or(false),
    })
}

#[tauri::command]
fn set_autostart(app: tauri::AppHandle, on: bool) -> bool {
    let launcher = app.autolaunch();
    let _ = if on { launcher.enable() } else { launcher.disable() };
    launcher.is_enabled().unwrap_or(false)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch (Start menu, autostart) focuses the running window instead of fighting for port 7070.
        .plugin(tauri_plugin_single_instance::init(|_, _, _| show_window()))
        // Started with Windows: come up in the tray, not in the user's face.
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .setup(|app| {
            let _ = APP.set(app.handle().clone());
            settings::init(app.path().app_config_dir()?);
            pairing::init(app.path().app_config_dir()?);
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
            get_settings,
            set_settings,
            options,
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
