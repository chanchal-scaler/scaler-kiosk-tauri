//! Window creation and kiosk behaviour that is the same on every platform.

use std::error::Error;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::webview::{NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse};
use tauri::{AppHandle, Manager, Monitor, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use url::Url;

use crate::{deeplink, platform, Config, KioskState};

/// Label of the exam window (referenced by `capabilities/*.json`).
pub const MAIN_WINDOW: &str = "main";

/// Script injected into every page before its own scripts run (see scripts/kiosk.js).
const INIT_SCRIPT: &str = include_str!("../scripts/kiosk.js");

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn Error>> {
    let config = app.state::<KioskState>().config.clone();
    log::info!(
        "Scaler Kiosk v{} starting on {} (kiosk={}, start_url={}, allowed_hosts={:?})",
        Config::version(),
        Config::platform(),
        config.kiosk,
        config.start_url,
        config.allowed_hosts
    );

    platform::install_menu(app)?;

    if config.kiosk && config.preflight_screen {
        platform::preflight_screen_capture();
    }

    // Lock the desktop down before the window exists so the borderless window is placed on a
    // screen that already has no menu bar or Dock.
    if config.kiosk {
        platform::enter_kiosk();
    }

    // A deep link the app was launched with replaces the configured start page.
    let state = app.state::<KioskState>();
    if let Some(target) = deeplink::launch_target(app, &state) {
        log::info!("launched with deep link → {target}");
        state.set_start_url(target);
    }
    let start_url = state.start_url();
    deeplink::install(app);

    let primary = app.primary_monitor()?;
    let window = build_main_window(app, &config, &start_url, primary.as_ref())?;

    if config.kiosk {
        if config.cover_secondary_displays {
            cover_secondary_displays(app, primary.as_ref())?;
        }
        window.set_focus()?;
    }

    register_exit_shortcut(app, &config)?;
    Ok(())
}

fn build_main_window(
    app: &AppHandle,
    config: &Config,
    start_url: &Url,
    primary: Option<&Monitor>,
) -> Result<WebviewWindow, Box<dyn Error>> {
    let page_config = serde_json::json!({
        "version": Config::version(),
        "platform": Config::platform(),
        "kiosk": config.kiosk,
        "startUrl": start_url.as_str(),
        "preflightMedia": config.preflight_media,
        "exitButton": config.kiosk && config.exit_button,
    });
    let init_script = INIT_SCRIPT.replace("__KIOSK_CONFIG__", &page_config.to_string());

    let popup_config = config.clone();
    let popup_app = app.clone();
    let mut builder = WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::App("index.html".into()))
        .title("Scaler")
        .user_agent(&config.user_agent())
        .initialization_script(init_script)
        // wry calls this for iframes as well as the main frame (no way to tell them apart on
        // macOS), so only schemes are filtered here; the host allowlist is enforced for the
        // main frame by `guard_page_load` below.
        .on_navigation(|url| {
            let allowed = Config::is_loadable(url);
            if !allowed {
                log::warn!("blocked navigation to {url}");
            }
            allowed
        })
        // window.open() / target="_blank": never spawn a second window; load the page in
        // the kiosk window instead (if the allowlist permits it).
        .on_new_window(move |url, _features| {
            if popup_config.is_navigation_allowed(&url) {
                log::info!("popup redirected into the kiosk window: {url}");
                if let Some(window) = popup_app.get_webview_window(MAIN_WINDOW) {
                    if let Err(err) = window.navigate(url) {
                        log::error!("failed to navigate to popup URL: {err}");
                    }
                }
            } else {
                log::warn!("blocked popup to {url}");
            }
            NewWindowResponse::Deny
        })
        // Camera is granted without a webview-level prompt (only allowlisted hosts can load
        // here, and the OS permission dialog still applies). The microphone is not used and is
        // always denied: the app has no microphone usage string, so it must never touch the
        // device. Everything else keeps the webview default.
        .on_permission_request(|_webview, kind| match kind {
            PermissionKind::Camera => PermissionResponse::Allow,
            // Linux (WebKitGTK) asks here before showing the portal's screen picker; the
            // candidate still chooses what to share. macOS/Windows don't route it through here.
            PermissionKind::DisplayCapture => PermissionResponse::Allow,
            PermissionKind::Microphone => {
                log::warn!("microphone requested by the page; denied (audio is not supported)");
                PermissionResponse::Deny
            }
            _ => PermissionResponse::Default,
        })
        .on_page_load(|webview, payload| {
            match payload.event() {
                PageLoadEvent::Started => log::info!("loading {}", payload.url()),
                PageLoadEvent::Finished => log::info!("loaded {}", payload.url()),
            }
            guard_page_load(&webview, payload.url(), payload.event());
        })
        .incognito(config.incognito)
        .zoom_hotkeys_enabled(false)
        .focused(true);

    if config.kiosk {
        builder = builder
            .decorations(false)
            .always_on_top(true)
            .resizable(false)
            .minimizable(false)
            .maximizable(false)
            .closable(false)
            .skip_taskbar(true)
            .visible_on_all_workspaces(true);

        if let Some(monitor) = primary {
            let (position, size) = monitor_rect(monitor);
            builder = builder
                .position(position.0, position.1)
                .inner_size(size.0, size.1);
        }

        // On macOS the borderless screen-sized window plus the presentation options in
        // platform/macos.rs *is* kiosk mode; native fullscreen would create a Space with
        // its own auto-revealing menu bar and Dock. Elsewhere, fullscreen is just a
        // borderless window covering the monitor, which is what we want.
        #[cfg(not(target_os = "macos"))]
        {
            builder = builder.fullscreen(true);
        }
    } else {
        builder = builder.inner_size(1280.0, 800.0).center();
    }

    let window = builder.build()?;
    platform::configure_webview(&window);
    Ok(window)
}

/// Main-frame allowlist. Page-load events only fire for the top-level page (never for
/// iframes), so this is where the host allowlist is enforced: when the window starts or
/// finishes loading a page outside it (a clicked link, a server redirect), it is sent back
/// to the last allowed page. `Finished` catches redirects that `Started` cannot see.
fn guard_page_load(webview: &WebviewWindow, url: &Url, event: PageLoadEvent) {
    let state = webview.state::<KioskState>();
    if state.config.is_navigation_allowed(url) {
        if event == PageLoadEvent::Finished {
            state.set_last_allowed(url.clone());
        }
        return;
    }
    let back = state.last_allowed().unwrap_or_else(|| state.start_url());
    log::warn!("blocked page {url} (not on the allowlist); returning to {back}");
    // Navigate from another thread so the request is queued on the event loop instead of
    // re-entering WebKit from inside its own navigation callback.
    let webview = webview.clone();
    std::thread::spawn(move || {
        if let Err(err) = webview.navigate(back) {
            log::error!("failed to leave blocked page: {err}");
        }
    });
}

/// Black out every display other than the primary one so a second monitor cannot be used
/// for anything else during the exam.
fn cover_secondary_displays(app: &AppHandle, primary: Option<&Monitor>) -> Result<(), Box<dyn Error>> {
    let primary_position = primary.map(|monitor| *monitor.position());
    for (index, monitor) in app.available_monitors()?.into_iter().enumerate() {
        let is_primary = match primary_position {
            Some(position) => *monitor.position() == position,
            None => index == 0,
        };
        if is_primary {
            continue;
        }
        let (position, size) = monitor_rect(&monitor);
        log::info!(
            "covering secondary display {} at ({}, {}) {}x{}",
            monitor.name().cloned().unwrap_or_default(),
            position.0,
            position.1,
            size.0,
            size.1
        );
        WebviewWindowBuilder::new(app, format!("cover-{index}"), WebviewUrl::App("cover.html".into()))
            .title("Scaler")
            .decorations(false)
            .always_on_top(true)
            .resizable(false)
            .minimizable(false)
            .maximizable(false)
            .closable(false)
            .skip_taskbar(true)
            .visible_on_all_workspaces(true)
            .focused(false)
            .position(position.0, position.1)
            .inner_size(size.0, size.1)
            .build()?;
    }
    Ok(())
}

/// Monitor origin and size in logical pixels, which is what the window builder expects.
fn monitor_rect(monitor: &Monitor) -> ((f64, f64), (f64, f64)) {
    let scale = monitor.scale_factor();
    let position = monitor.position().to_logical::<f64>(scale);
    let size = monitor.size().to_logical::<f64>(scale);
    ((position.x, position.y), (size.width, size.height))
}

fn register_exit_shortcut(app: &AppHandle, config: &Config) -> Result<(), Box<dyn Error>> {
    let combo = config.exit_shortcut.trim();
    if combo.is_empty() {
        log::warn!("EXIT_SHORTCUT is empty: the only way out is a page calling exit_kiosk()");
        return Ok(());
    }
    let registered = app.global_shortcut().on_shortcut(combo, |app, _shortcut, event| {
        if event.state() == ShortcutState::Pressed {
            log::info!("exit shortcut pressed");
            confirm_exit(app);
        }
    });
    match registered {
        Ok(()) => log::info!("exit shortcut registered: {combo}"),
        // A bad combination must not take the whole app down; a page can still call exit_kiosk().
        Err(err) => log::error!("cannot register exit shortcut {combo:?}: {err}"),
    }
    Ok(())
}

/// Asks the operator to confirm before quitting (the shortcut is easy to hit by accident
/// during an exam). The dialog is non-blocking because the shortcut handler runs on the
/// main thread.
pub fn confirm_exit(app: &AppHandle) {
    let state = app.state::<KioskState>();
    if state.confirming_exit.swap(true, Ordering::SeqCst) {
        return; // a dialog is already open
    }
    let handle = app.clone();
    app.dialog()
        .message("Quit Scaler Kiosk? Any exam in progress will be interrupted.")
        .title("Exit kiosk")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom("Exit kiosk".into(), "Cancel".into()))
        .show(move |confirmed| {
            handle.state::<KioskState>().confirming_exit.store(false, Ordering::SeqCst);
            if confirmed {
                exit(&handle);
            } else {
                log::info!("exit cancelled");
            }
        });
}

/// Unlocks the kiosk and quits. Safe to call from any thread.
pub fn exit(app: &AppHandle) {
    app.state::<KioskState>().unlock();
    let _ = app.run_on_main_thread(platform::leave_kiosk);
    app.exit(0);
}

/// Pulls focus back a moment after it was lost. The delay gives OS dialogs (permission
/// prompts, the screen-share picker) a chance to be answered first.
pub fn regain_focus(window: &Window) {
    let window = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        if !window.is_focused().unwrap_or(true) {
            log::info!("focus lost; bringing the kiosk window back");
            let _ = window.set_focus();
        }
    });
}
