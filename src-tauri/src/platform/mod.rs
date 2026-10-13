//! OS-specific lockdown. Each platform module exposes the same four functions:
//!
//! - `install_menu`            – application menu (macOS needs one for Cmd+C/V to work)
//! - `enter_kiosk`             – hide Dock/menu bar, block app switching, force quit, …
//! - `leave_kiosk`             – undo `enter_kiosk` (called right before quitting)
//! - `preflight_screen_capture` – trigger the Screen Recording permission prompt early
//! - `configure_webview`       – engine settings the exam pages need (Linux: enable media)

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
pub use other::*;
