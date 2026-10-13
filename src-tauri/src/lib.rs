//! Scaler Kiosk: runs scaler.com inside a locked-down Tauri webview.
//!
//! Layout:
//! - `config`   – runtime configuration (start URL, allowlist, env overrides)
//! - `kiosk`    – window creation, navigation allowlist, exit shortcut, secondary-display covers
//! - `platform` – OS-specific lockdown (macOS presentation options, menu, permission preflight)
//! - `commands` – IPC commands the launcher page and scaler.com pages may call
//! - `deeplink` – `scaler-kiosk://` links that open the kiosk on a specific page

mod commands;
mod config;
mod deeplink;
mod kiosk;
mod platform;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

use tauri::{Manager, RunEvent, WindowEvent};
use url::Url;

pub use config::Config;

/// Shared application state: the resolved configuration plus whether the kiosk has been
/// unlocked for exit (by the exit shortcut or by a page calling `exit_kiosk`).
pub struct KioskState {
    pub config: Config,
    /// Page the launcher hands over to: the configured start URL, or the deep link the app
    /// was opened with.
    start_url: RwLock<Url>,
    /// Last allowed page the main window finished loading; where the page-load guard sends
    /// the window back to when it ends up somewhere off the allowlist.
    last_allowed: RwLock<Option<Url>>,
    unlocked: AtomicBool,
    /// Set while the "Exit kiosk?" confirmation dialog is open.
    pub(crate) confirming_exit: AtomicBool,
}

impl KioskState {
    fn new(config: Config) -> Self {
        Self {
            start_url: RwLock::new(config.start_url.clone()),
            last_allowed: RwLock::new(None),
            config,
            unlocked: AtomicBool::new(false),
            confirming_exit: AtomicBool::new(false),
        }
    }

    pub fn start_url(&self) -> Url {
        self.start_url.read().expect("start_url lock poisoned").clone()
    }

    pub fn set_start_url(&self, url: Url) {
        *self.start_url.write().expect("start_url lock poisoned") = url;
    }

    pub fn last_allowed(&self) -> Option<Url> {
        self.last_allowed.read().expect("last_allowed lock poisoned").clone()
    }

    pub fn set_last_allowed(&self, url: Url) {
        *self.last_allowed.write().expect("last_allowed lock poisoned") = Some(url);
    }

    /// Allow the application to close and exit.
    pub fn unlock(&self) {
        self.unlocked.store(true, Ordering::SeqCst);
    }

    /// True while the window must stay up: kiosk mode is on and nobody has unlocked it.
    pub fn is_locked(&self) -> bool {
        self.config.kiosk && !self.unlocked.load(Ordering::SeqCst)
    }
}

pub fn run() {
    let context = tauri::generate_context!();
    let mut config = Config::from_env();
    // Under `tauri dev` the bundled pages come from the CLI's dev server, not tauri://localhost.
    config.allow_dev_server(context.config().build.dev_url.as_ref());

    let app = tauri::Builder::default()
        // Must be the first plugin: a second launch (Windows/Linux deep links start a new
        // process) hands its arguments to the running instance and exits.
        .plugin(tauri_plugin_single_instance::init(|_app, args, _cwd| {
            log::info!("another instance was started with {args:?}; forwarded here");
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("kiosk".into()),
                    }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(KioskState::new(config))
        .invoke_handler(tauri::generate_handler![
            commands::exit_kiosk,
            commands::kiosk_info,
            commands::kiosk_log
        ])
        .setup(|app| {
            kiosk::setup(app.handle())?;
            Ok(())
        })
        .on_window_event(|window, event| {
            let state = window.state::<KioskState>();
            match event {
                // The close button is hidden in kiosk mode, but Cmd+W / Alt+F4 style requests
                // still arrive here: swallow them while locked.
                WindowEvent::CloseRequested { api, .. } if state.is_locked() => {
                    log::warn!("close requested for '{}' while locked; ignoring", window.label());
                    api.prevent_close();
                }
                WindowEvent::Focused(false) if state.is_locked() && state.config.refocus => {
                    kiosk::regain_focus(window);
                }
                _ => {}
            }
        })
        .build(context)
        .expect("error while building Scaler Kiosk");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, .. } = &event {
            if app.state::<KioskState>().is_locked() {
                log::warn!("exit requested while locked; ignoring");
                api.prevent_exit();
            }
        }
    });
}
