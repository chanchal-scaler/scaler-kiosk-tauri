fn main() {
    // Declare the app's own IPC commands so Tauri generates `allow-<command>` permissions
    // for them. The capabilities in `capabilities/` then grant those to the bundled launcher
    // page and to pages served from scaler.com (see capabilities/scaler.json).
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&["exit_kiosk", "request_exit", "kiosk_info", "kiosk_log"]),
        ),
    )
    .expect("failed to run tauri-build");
}
