//! Windows and Linux: the borderless, always-on-top fullscreen window created in
//! `kiosk.rs` is the whole kiosk. There is no equivalent of macOS presentation options;
//! OS-level lockdown (Assigned Access on Windows, a kiosk session on Linux) is a
//! deployment concern rather than something the app can do by itself.

use std::error::Error;

use tauri::{AppHandle, WebviewWindow};

pub fn install_menu(_app: &AppHandle) -> Result<(), Box<dyn Error>> {
    // No menu bar on Windows/Linux.
    Ok(())
}

pub fn enter_kiosk() {}

pub fn leave_kiosk() {}

pub fn preflight_screen_capture() {
    // WebView2 (Windows) and WebKitGTK (Linux, via xdg-desktop-portal) show their own share
    // pickers on getDisplayMedia(); nothing to do up front.
}

/// WebView2 (Windows) has camera and screen capture enabled by default.
#[cfg(not(target_os = "linux"))]
pub fn configure_webview(_window: &WebviewWindow) {}

/// WebKitGTK has getUserMedia, WebRTC and getDisplayMedia disabled by default and wry leaves
/// them that way, so the camera would silently fail on Linux without this.
#[cfg(target_os = "linux")]
pub fn configure_webview(window: &WebviewWindow) {
    let result = window.with_webview(|webview| {
        use webkit2gtk::{SettingsExt, WebViewExt};
        match webview.inner().settings() {
            Some(settings) => {
                settings.set_enable_media_stream(true);
                settings.set_enable_webrtc(true);
                settings.set_enable_mediasource(true);
                log::info!("WebKitGTK media stream and WebRTC enabled");
            }
            None => log::error!("WebKitGTK settings unavailable; camera will not work"),
        }
    });
    if let Err(err) = result {
        log::error!("could not configure WebKitGTK: {err}");
    }
}
