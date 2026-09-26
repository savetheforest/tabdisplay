// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `--install-driver` & co. run elevated from the installer or a UAC prompt, without the UI.
    #[cfg(windows)]
    if let Some(code) = std::env::args().nth(1).and_then(|arg| tabdisplay_lib::driver_cli(&arg)) {
        std::process::exit(code);
    }
    tabdisplay_lib::run()
}
