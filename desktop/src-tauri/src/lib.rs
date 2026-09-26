mod encode;
mod server;
mod settings;
#[cfg(windows)]
mod win {
    pub mod capture;
    pub mod display;
    pub mod driver;
    pub mod encode;
    pub mod input;
}

use settings::Settings;
use std::net::UdpSocket;
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::Manager;
use win::{capture, display, driver};

pub use win::driver::cli as driver_cli;

/// Last result of the automatic `adb reverse`, shown in the UI.
static USB: Mutex<&str> = Mutex::new("");

/// (LAN IP, server status, virtual display driver state, USB state) for the UI.
#[tauri::command]
fn status() -> (String, String, &'static str, &'static str) {
    // Connecting a UDP socket sends nothing; it just makes the OS pick the outbound interface.
    let ip = UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("8.8.8.8:80").and(s.local_addr()))
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "?".into());
    (ip, server::STATUS.lock().unwrap().clone(), display::driver_state(), *USB.lock().unwrap())
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

/// adb from PATH if there is one, so we don't fight a different adb version (e.g. Android Studio's)
/// over the shared adb server; otherwise the bundled copy.
fn adb(args: &[&str]) -> std::io::Result<std::process::Output> {
    static PATH: OnceLock<std::path::PathBuf> = OnceLock::new();
    let path = PATH.get_or_init(|| {
        let on_path = hidden(Command::new("adb").arg("version")).output().is_ok();
        if on_path { "adb".into() } else { driver::resource(r"adb\adb.exe") }
    });
    hidden(Command::new(path).args(args)).output()
}

fn hidden(cmd: &mut Command) -> &mut Command {
    std::os::windows::process::CommandExt::creation_flags(cmd, 0x0800_0000) // CREATE_NO_WINDOW
}

/// Keeps `adb reverse` set up so a tablet on the cable can reach this PC at its 127.0.0.1.
// ponytail: polls adb every 3s; switch to `adb track-devices` if the process spawning ever matters.
fn usb_forward() {
    let port = format!("tcp:{}", server::PORT);
    loop {
        let ok = adb(&["reverse", &port, &port]).is_ok_and(|o| o.status.success());
        *USB.lock().unwrap() = if ok { "Cabo USB: tablet pronto (toque em USB no tablet)" } else { "" };
        std::thread::sleep(Duration::from_secs(3));
    }
}

/// Installs the bundled tablet app over the cable (USB debugging must be on).
#[tauri::command]
async fn install_apk() -> String {
    let apk = driver::resource("tabdisplay.apk");
    match adb(&["install", "-r", &apk.display().to_string()]) {
        Ok(o) if o.status.success() => "App instalado no tablet.".into(),
        Ok(o) => format!("Falhou: {}", String::from_utf8_lossy(&o.stdout).trim()),
        Err(e) => format!("adb indisponível ({e})"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            settings::init(app.path().app_config_dir()?);
            let _ = display::ensure_modes(&[]);
            std::thread::spawn(server::run);
            std::thread::spawn(server::beacon);
            std::thread::spawn(usb_forward);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![status, get_settings, set_settings, options, restart_driver, install_apk])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
