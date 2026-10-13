//! Deep links: open the kiosk straight on a page, e.g. the link a candidate receives for a
//! test. Two forms are accepted:
//!
//! - `scaler-kiosk://www.scaler.com/test/abc123` – the `https://` URL with its scheme swapped,
//!   which is how SEB's `sebs://` links work (any scaler.com URL becomes a kiosk link by
//!   replacing `https://` with `scaler-kiosk://`).
//! - `scaler-kiosk://open?url=https%3A%2F%2Fwww.scaler.com%2Ftest%2Fabc123` – explicit form.
//!
//! The target must pass the navigation allowlist (`Config::is_navigation_allowed`); anything
//! else is ignored and logged, so a link cannot push the kiosk to an arbitrary site.
//!
//! The scheme is declared in tauri.conf.json (`plugins.deep-link`). macOS registers it
//! through the bundled `.app`, Windows through the installer; on Linux the app registers a
//! per-user handler itself at start-up (see `install`).

use tauri::{AppHandle, Manager};
use tauri_plugin_deep_link::DeepLinkExt;
use url::Url;

use crate::{kiosk, KioskState};

/// URL scheme candidates' links use (also listed in tauri.conf.json).
pub const SCHEME: &str = "scaler-kiosk";

/// Translates a deep link (or a plain page URL given on the command line) into the page it
/// points at. Returns `None` for anything that is not an http(s) page.
pub fn target_url(link: &Url) -> Option<Url> {
    match link.scheme() {
        "https" | "http" => Some(link.clone()),
        s if s == SCHEME => {
            if link.host_str() == Some("open") {
                return link
                    .query_pairs()
                    .find(|(key, _)| key == "url")
                    .and_then(|(_, value)| Url::parse(&value).ok())
                    .filter(|url| matches!(url.scheme(), "https" | "http"));
            }
            let rest = link.as_str().strip_prefix(&format!("{SCHEME}://"))?;
            Url::parse(&format!("https://{rest}")).ok()
        }
        _ => None,
    }
}

/// The first link whose target the kiosk is allowed to show.
fn first_allowed<I: IntoIterator<Item = Url>>(links: I, state: &KioskState) -> Option<Url> {
    links.into_iter().find_map(|link| match target_url(&link) {
        Some(target) if state.config.is_navigation_allowed(&target) => Some(target),
        _ => {
            log::warn!("ignoring deep link {link}: not an allowed page");
            None
        }
    })
}

/// Page the app was launched for, if any: a deep link from Windows/Linux command-line
/// arguments, or a plain page URL passed as the last argument (handy in development:
/// `npm run dev -- -- https://www.scaler.com/test/abc123`). On macOS the launch URL arrives
/// as an event a moment after start-up and is handled by [`install`].
pub fn launch_target(app: &AppHandle, state: &KioskState) -> Option<Url> {
    let from_plugin = app
        .deep_link()
        .get_current()
        .ok()
        .flatten()
        .unwrap_or_default();
    let from_args = std::env::args()
        .skip(1)
        .filter_map(|arg| Url::parse(&arg).ok());
    first_allowed(from_plugin.into_iter().chain(from_args), state)
}

/// Listens for deep links delivered while the app is running (or at launch on macOS).
pub fn install(app: &AppHandle) {
    // Linux: always (re)register at start-up. The bundler's .desktop file declares the
    // scheme but its Exec line has no %u, so the link would never reach the app; this writes
    // a per-user handler with `"<exe>" %u` (the AppImage's real path when run as one).
    // Windows: the installer registers the scheme; only point it at the dev binary in dev.
    #[cfg(any(windows, target_os = "linux"))]
    if cfg!(target_os = "linux") || cfg!(debug_assertions) {
        if let Err(err) = app.deep_link().register_all() {
            log::warn!("could not register the {SCHEME}:// scheme: {err}");
        }
    }

    let handle = app.clone();
    app.deep_link().on_open_url(move |event| {
        let state = handle.state::<KioskState>();
        if let Some(target) = first_allowed(event.urls(), &state) {
            open(&handle, target);
        }
    });
}

/// Shows `target` in the kiosk window. If the launcher page is still up it will pick the
/// new start URL up itself (it asks for it right before navigating); otherwise navigate now.
pub fn open(app: &AppHandle, target: Url) {
    log::info!("deep link → {target}");
    let state = app.state::<KioskState>();
    state.set_start_url(target.clone());
    let Some(window) = app.get_webview_window(kiosk::MAIN_WINDOW) else {
        return;
    };
    let on_launcher = window
        .url()
        .map(|current| state.config.is_bundled_page(&current))
        .unwrap_or(false);
    if !on_launcher {
        if let Err(err) = window.navigate(target) {
            log::error!("failed to open deep link: {err}");
        }
    }
    let _ = window.set_focus();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(link: &str) -> Option<String> {
        target_url(&Url::parse(link).unwrap()).map(|u| u.to_string())
    }

    #[test]
    fn swaps_the_scheme() {
        assert_eq!(
            target("scaler-kiosk://www.scaler.com/test/abc123?x=1#q2").as_deref(),
            Some("https://www.scaler.com/test/abc123?x=1#q2")
        );
        assert_eq!(
            target("scaler-kiosk://www.scaler.com").as_deref(),
            Some("https://www.scaler.com/")
        );
    }

    #[test]
    fn accepts_the_explicit_form() {
        assert_eq!(
            target("scaler-kiosk://open?url=https%3A%2F%2Fwww.scaler.com%2Ftest%2Fabc123").as_deref(),
            Some("https://www.scaler.com/test/abc123")
        );
        assert_eq!(target("scaler-kiosk://open?url=javascript%3Aalert(1)"), None);
        assert_eq!(target("scaler-kiosk://open"), None);
    }

    #[test]
    fn passes_plain_pages_and_rejects_the_rest() {
        assert_eq!(
            target("https://www.scaler.com/test/1").as_deref(),
            Some("https://www.scaler.com/test/1")
        );
        assert_eq!(target("mailto:someone@scaler.com"), None);
        assert_eq!(target("file:///etc/passwd"), None);
    }
}
