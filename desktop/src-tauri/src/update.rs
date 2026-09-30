//! Self-update through tauri-plugin-updater. The app asks the endpoints in `plugins.updater` (tauri.conf.json,
//! a `latest.json` on GitHub Releases) for a newer version, downloads it and checks its minisign signature
//! against the public key baked into the app before running anything; a wrong signature aborts the update.
use serde_json::{json, Value};
use std::sync::Mutex;
use tauri_plugin_updater::{Update, UpdaterExt};

/// The update `check_update` found, waiting for `install_update`.
static PENDING: Mutex<Option<Update>> = Mutex::new(None);

/// `null` when this is the latest version, else `{version, notes}`.
#[tauri::command]
pub async fn check_update(app: tauri::AppHandle) -> Result<Value, String> {
    let found = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    let info = found
        .as_ref()
        .map(|u| json!({ "version": u.version, "notes": u.body }));
    *PENDING.lock().unwrap() = found;
    Ok(info.unwrap_or(Value::Null))
}

/// Downloads, verifies and installs the update `check_update` found, then restarts the app.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    let update = PENDING
        .lock()
        .unwrap()
        .take()
        .ok_or("Nenhuma atualização encontrada: verifique de novo.")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}
