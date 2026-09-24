mod encode;
mod server;
#[cfg(windows)]
mod win {
    pub mod capture;
    pub mod input;
}

use std::net::UdpSocket;
use std::process::Command;

/// (LAN IP, server status) for the UI.
#[tauri::command]
fn status() -> (String, String) {
    // Connecting a UDP socket sends nothing; it just makes the OS pick the outbound interface.
    let ip = UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| s.connect("8.8.8.8:80").and(s.local_addr()))
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "?".into());
    (ip, server::STATUS.lock().unwrap().clone())
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
    std::thread::spawn(server::run);
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![status, adb_reverse])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
