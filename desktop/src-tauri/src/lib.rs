mod encode;
mod server;
mod settings;
#[cfg(windows)]
mod win {
    pub mod capture;
    pub mod display;
    pub mod encode;
    pub mod input;
}

use settings::Settings;
use std::net::UdpSocket;
use std::process::Command;
use tauri::Manager;
use win::{capture, display};

/// (LAN IP, server status, virtual display driver state) for the UI.
#[tauri::command]
fn status() -> (String, String, &'static str) {
    // Connecting a UDP socket sends nothing; it just makes the OS pick the outbound interface.
    let ip = UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("8.8.8.8:80").and(s.local_addr()))
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "?".into());
    (ip, server::STATUS.lock().unwrap().clone(), display::driver_state())
}

#[tauri::command]
fn get_settings() -> Settings {
    settings::get()
}

#[tauri::command]
fn set_settings(settings: Settings) {
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
    let _ = display::ensure_modes(&[]);
    match display::restart_driver() {
        Ok(()) => format!("Driver: {}", display::driver_state()),
        Err(e) => format!("Falhou: {e}"),
    }
}

/// Forwards the tablet's localhost:PORT to this PC over USB.
#[tauri::command]
fn adb_reverse() -> String {
    let port = format!("tcp:{}", server::PORT);
    let mut cmd = Command::new("adb");
    cmd.args(["reverse", &port, &port]);
    #[cfg(windows)]
    std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // CREATE_NO_WINDOW
    match cmd.output() {
        Ok(o) if o.status.success() => "USB pronto. No tablet, toque em USB.".into(),
        Ok(o) => format!("adb: {}", String::from_utf8_lossy(&o.stderr).trim()),
        Err(e) => format!("adb não encontrado no PATH ({e})"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            settings::init(app.path().app_config_dir()?);
            let _ = display::ensure_modes(&[]);
            std::thread::spawn(server::run);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![status, get_settings, set_settings, options, restart_driver, adb_reverse])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
