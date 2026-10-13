//! IPC commands exposed to the launcher page and to scaler.com pages.
//!
//! Which page may call what is declared in `capabilities/*.json`; the `allow-*`
//! permissions are generated from the command list in `build.rs`.

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{kiosk, Config, KioskState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KioskInfo {
    name: &'static str,
    version: &'static str,
    platform: &'static str,
    kiosk: bool,
    start_url: String,
    allowed_hosts: Vec<String>,
}

/// `window.scalerKiosk.info()` – lets a page confirm it runs inside the kiosk and
/// which build it is talking to.
#[tauri::command]
pub fn kiosk_info(state: State<'_, KioskState>) -> KioskInfo {
    KioskInfo {
        name: "ScalerKiosk",
        version: Config::version(),
        platform: Config::platform(),
        kiosk: state.config.kiosk,
        start_url: state.start_url().to_string(),
        allowed_hosts: state.config.allowed_hosts.clone(),
    }
}

/// `window.scalerKiosk.exit()` – unlocks and quits. Meant to be called by scaler.com
/// once the exam has been submitted.
#[tauri::command]
pub fn exit_kiosk(app: AppHandle) {
    log::info!("exit_kiosk invoked by page");
    kiosk::exit(&app);
}

/// `window.scalerKiosk.log(level, message)` – lets pages write to the kiosk log file
/// (useful for support: "send us the kiosk log").
#[tauri::command]
pub fn kiosk_log(level: String, message: String) {
    let message = message.chars().take(2000).collect::<String>();
    match level.to_ascii_lowercase().as_str() {
        "error" => log::error!(target: "page", "{message}"),
        "warn" | "warning" => log::warn!(target: "page", "{message}"),
        "debug" => log::debug!(target: "page", "{message}"),
        _ => log::info!(target: "page", "{message}"),
    }
}
