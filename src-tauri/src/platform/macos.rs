//! macOS lockdown: NSApplication presentation options (the same mechanism Apple's kiosk
//! guidance and Safe Exam Browser use), a minimal menu, and the Screen Recording preflight.

use std::error::Error;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationPresentationOptions};
use tauri::menu::{AboutMetadata, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::AppHandle;

use crate::kiosk;

/// Menu id of the "Quit Scaler Kiosk" item.
const QUIT_ITEM: &str = "kiosk-quit";

/// Replaces Tauri's default menu. Cmd+Q is a custom "Quit" item that asks for confirmation
/// (the stock Quit item would terminate straight away); there is deliberately no Hide,
/// Minimize or Close item, so Cmd+H / Cmd+M / Cmd+W do nothing. The Edit items stay so that
/// Cmd+C / Cmd+V / Cmd+A keep working inside the webview (key equivalents on macOS are
/// routed through the menu, even while the menu bar is hidden).
pub fn install_menu(app: &AppHandle) -> Result<(), Box<dyn Error>> {
    let quit = MenuItemBuilder::with_id(QUIT_ITEM, "Quit Scaler Kiosk")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;
    let app_menu = SubmenuBuilder::new(app, "Scaler Kiosk")
        .about(Some(AboutMetadata::default()))
        .separator()
        .item(&quit)
        .build()?;
    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;
    let menu = MenuBuilder::new(app).items(&[&app_menu, &edit_menu]).build()?;
    app.set_menu(menu)?;
    app.on_menu_event(|app, event| {
        if event.id() == QUIT_ITEM {
            log::info!("Cmd+Q pressed");
            kiosk::confirm_exit(app);
        }
    });
    Ok(())
}

/// Hide the Dock and menu bar and disable Cmd+Tab, force quit (Cmd+Opt+Esc), log out /
/// shut down and Hide. Must run on the main thread (it does during `setup`).
pub fn enter_kiosk() {
    set_presentation_options(
        NSApplicationPresentationOptions::HideDock
            | NSApplicationPresentationOptions::HideMenuBar
            | NSApplicationPresentationOptions::DisableAppleMenu
            | NSApplicationPresentationOptions::DisableProcessSwitching
            | NSApplicationPresentationOptions::DisableForceQuit
            | NSApplicationPresentationOptions::DisableSessionTermination
            | NSApplicationPresentationOptions::DisableHideApplication,
    );
}

pub fn leave_kiosk() {
    set_presentation_options(NSApplicationPresentationOptions::Default);
}

fn set_presentation_options(options: NSApplicationPresentationOptions) {
    let Some(mtm) = MainThreadMarker::new() else {
        log::error!("presentation options can only be changed on the main thread");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    // AppKit raises an exception for combinations it considers invalid; the ones used in
    // this file are the documented kiosk set.
    app.setPresentationOptions(options);
    log::info!("presentation options set to {options:?}");
}

/// Screen Recording permission (needed for getDisplayMedia) has no Info.plist usage string;
/// macOS prompts on first use and may require the app to be relaunched. Asking at start-up
/// means the candidate sees that dialog before the exam, not in the middle of it.
pub fn preflight_screen_capture() {
    // SAFETY: plain CoreGraphics calls with no arguments.
    let granted = unsafe {
        if CGPreflightScreenCaptureAccess() {
            true
        } else {
            CGRequestScreenCaptureAccess()
        }
    };
    if granted {
        log::info!("screen recording permission: granted");
    } else {
        log::warn!(
            "screen recording permission not granted; enable it in System Settings > Privacy & Security > Screen Recording and relaunch"
        );
    }
}

/// WKWebView needs no extra settings: camera and screen capture work out of the box.
pub fn configure_webview(_window: &tauri::WebviewWindow) {}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGRequestScreenCaptureAccess() -> bool;
}
