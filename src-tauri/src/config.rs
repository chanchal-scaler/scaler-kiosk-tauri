//! Runtime configuration.
//!
//! Everything has a compiled-in default; environment variables override it, which is
//! handy during development (`KIOSK=0 APP_URL=http://localhost:3000 npm run dev`).
//! The variable names match the Electron scaffold in ../scaler-kiosk.
//! A bundled app launched from Finder/Explorer sees no environment and uses the defaults.

use std::env;

use url::Url;

/// Page the kiosk opens after the launcher has confirmed connectivity.
pub const DEFAULT_START_URL: &str = "https://www.scaler.com/";

/// Hosts the main window may show (subdomains included). Enforced for top-level pages by
/// the page-load guard in `kiosk.rs`; iframes and sub-resources (Cloudflare Turnstile,
/// YouTube embeds, CDNs, APIs) are not restricted. Google's country sign-in domains
/// (`accounts.google.co.in`, …) are matched separately, see `is_google_accounts_host`.
pub const DEFAULT_ALLOWED_HOSTS: &[&str] = &[
    "scaler.com",
    "interviewbit.com",
    // Google sign-in bounces through these.
    "accounts.google.com",
    "accounts.youtube.com",
];

/// Hidden key combination that quits the kiosk (parsed by tauri-plugin-global-shortcut).
pub const DEFAULT_EXIT_SHORTCUT: &str = "Control+Alt+Shift+Q";

/// Programs killed when kiosk mode starts (see `kill_blocked_apps` in `kiosk.rs`). Matched
/// case-insensitively against the executable name, so `claude` covers the Claude desktop
/// app (`Claude`, `Claude.exe`) and the Claude Code CLI (`claude`, `node …/claude`).
pub const DEFAULT_BLOCKED_APPS: &[&str] = &["claude"];

/// Token appended to the User-Agent so scaler.com can recognise the kiosk server-side.
pub const USER_AGENT_TOKEN: &str = "ScalerKiosk";

#[derive(Clone, Debug)]
pub struct Config {
    /// `APP_URL` – first page to open.
    pub start_url: Url,
    /// `KIOSK` – full lockdown (default) or a normal resizable window for development.
    pub kiosk: bool,
    /// `ALLOWED_HOSTS` – comma-separated extra hosts; `*` disables the allowlist.
    pub allowed_hosts: Vec<String>,
    /// Derived from `ALLOWED_HOSTS=*`.
    pub allow_all_hosts: bool,
    /// `EXIT_SHORTCUT` – key combination that quits (after confirmation); empty disables it.
    pub exit_shortcut: String,
    /// `KIOSK_REFOCUS` – pull focus back when another window steals it (off by default:
    /// it can fight with OS permission dialogs and the screen-share picker).
    pub refocus: bool,
    /// `KIOSK_INCOGNITO` – throw away cookies/storage when the app quits.
    pub incognito: bool,
    /// `KIOSK_PREFLIGHT_MEDIA` – launcher asks for the camera before opening scaler.com.
    pub preflight_media: bool,
    /// `KIOSK_PREFLIGHT_SCREEN` – macOS: trigger the Screen Recording prompt at start-up.
    pub preflight_screen: bool,
    /// `KIOSK_COVER_DISPLAYS` – black out every display except the primary one.
    pub cover_secondary_displays: bool,
    /// `KIOSK_EXIT_BUTTON` – show the small "Exit" button in the top-right corner.
    pub exit_button: bool,
    /// `KIOSK_BLOCKED_APPS` – comma-separated program names killed at start-up (kiosk mode
    /// only); empty disables it.
    pub blocked_apps: Vec<String>,
    /// Origin of Tauri's dev server under `tauri dev` (see `allow_dev_server`).
    pub dev_server: Option<Url>,
}

impl Config {
    pub fn from_env() -> Self {
        let start_url = env::var("APP_URL")
            .ok()
            .and_then(|raw| match Url::parse(raw.trim()) {
                Ok(url) if url.host_str().is_some() => Some(url),
                _ => {
                    eprintln!("APP_URL is not a valid absolute URL; using {DEFAULT_START_URL}");
                    None
                }
            })
            .unwrap_or_else(|| Url::parse(DEFAULT_START_URL).expect("default start URL is valid"));

        let kiosk = env_flag("KIOSK", true);

        let extra_hosts = env::var("ALLOWED_HOSTS").unwrap_or_default();
        let allow_all_hosts = extra_hosts.trim() == "*";
        let mut allowed_hosts: Vec<String> = DEFAULT_ALLOWED_HOSTS
            .iter()
            .map(|host| host.to_string())
            .collect();
        if !allow_all_hosts {
            allowed_hosts.extend(
                extra_hosts
                    .split(',')
                    .map(|host| host.trim().to_ascii_lowercase())
                    .filter(|host| !host.is_empty()),
            );
        }
        // The start page itself is always reachable (e.g. APP_URL=http://localhost:3000).
        if let Some(host) = start_url.host_str() {
            let host = host.to_ascii_lowercase();
            if !allowed_hosts.contains(&host) {
                allowed_hosts.push(host);
            }
        }

        Self {
            start_url,
            kiosk,
            allowed_hosts,
            allow_all_hosts,
            exit_shortcut: env::var("EXIT_SHORTCUT")
                .unwrap_or_else(|_| DEFAULT_EXIT_SHORTCUT.to_string()),
            refocus: env_flag("KIOSK_REFOCUS", false),
            incognito: env_flag("KIOSK_INCOGNITO", false),
            preflight_media: env_flag("KIOSK_PREFLIGHT_MEDIA", true),
            preflight_screen: env_flag("KIOSK_PREFLIGHT_SCREEN", kiosk),
            cover_secondary_displays: env_flag("KIOSK_COVER_DISPLAYS", kiosk),
            exit_button: env_flag("KIOSK_EXIT_BUTTON", true),
            blocked_apps: env::var("KIOSK_BLOCKED_APPS")
                .map(|raw| {
                    raw.split(',')
                        .map(|name| name.trim().to_string())
                        .filter(|name| !name.is_empty())
                        .collect()
                })
                .unwrap_or_else(|_| DEFAULT_BLOCKED_APPS.iter().map(|name| name.to_string()).collect()),
            dev_server: None,
        }
    }

    /// `tauri dev` serves the bundled pages from its own dev server (http://127.0.0.1:1430 by
    /// default) instead of `tauri://localhost`, so that origin must be reachable too.
    /// Release builds have no dev URL and this is a no-op.
    pub fn allow_dev_server(&mut self, dev_url: Option<&Url>) {
        if let Some(url) = dev_url {
            if let Some(host) = url.host_str() {
                let host = host.to_ascii_lowercase();
                if !self.allowed_hosts.contains(&host) {
                    self.allowed_hosts.push(host);
                }
            }
            self.dev_server = Some(url.clone());
        }
    }

    /// True for the app's own pages (launcher, display covers): `tauri://localhost` on
    /// macOS/Linux, `http://tauri.localhost` on Windows, or the dev server under `tauri dev`.
    pub fn is_bundled_page(&self, url: &Url) -> bool {
        url.scheme() == "tauri"
            || url.host_str() == Some("tauri.localhost")
            || self
                .dev_server
                .as_ref()
                .is_some_and(|dev| dev.origin() == url.origin())
    }

    pub fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    pub fn platform() -> &'static str {
        env::consts::OS
    }

    /// A Safari/Chromium-looking User-Agent (so sign-in providers treat us like a real
    /// browser) with the `ScalerKiosk/<version>` token appended for server-side detection.
    pub fn user_agent(&self) -> String {
        let base = if cfg!(target_os = "macos") {
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15"
        } else if cfg!(target_os = "windows") {
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0"
        } else {
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15"
        };
        format!("{base} {USER_AGENT_TOKEN}/{}", Self::version())
    }

    /// Anything a frame may load at all: web pages and the app's own pages. Applied to every
    /// navigation, iframes included (WKWebView cannot tell them apart in wry), so it must
    /// not be host-based; it only stops `file:` and other local or custom schemes.
    pub fn is_loadable(url: &Url) -> bool {
        matches!(url.scheme(), "http" | "https" | "tauri" | "about" | "blob" | "data")
    }

    /// Allowlist for what the main window may *show*: bundled pages always, otherwise only
    /// the allowed hosts and their subdomains.
    pub fn is_navigation_allowed(&self, url: &Url) -> bool {
        match url.scheme() {
            // Bundled launcher/cover pages: tauri://localhost (macOS, Linux) …
            "tauri" | "about" | "blob" => true,
            "http" | "https" => {
                let Some(host) = url.host_str() else {
                    return false;
                };
                let host = host.to_ascii_lowercase();
                // … and http://tauri.localhost on Windows.
                if host == "tauri.localhost" {
                    return true;
                }
                if self.allow_all_hosts || is_google_accounts_host(&host) {
                    return true;
                }
                self.allowed_hosts
                    .iter()
                    .any(|allowed| host == *allowed || host.ends_with(&format!(".{allowed}")))
            }
            _ => false,
        }
    }
}

/// Google sign-in sets its cookies on the country domain too, e.g. `accounts.google.co.in`
/// after `accounts.google.com`. Matches `accounts.google.<tld>` where the TLD is one or two
/// short alphabetic labels (`com`, `de`, `co.in`, `com.br`), not arbitrary subdomains.
fn is_google_accounts_host(host: &str) -> bool {
    let Some(tld) = host.strip_prefix("accounts.google.") else {
        return false;
    };
    let labels: Vec<&str> = tld.split('.').collect();
    (1..=2).contains(&labels.len())
        && labels
            .iter()
            .all(|label| (2..=3).contains(&label.len()) && label.chars().all(|c| c.is_ascii_alphabetic()))
}

/// Reads a boolean environment flag. `0`, `false`, `no`, `off` mean false; anything else
/// means true; unset (or empty) keeps the default.
fn env_flag(name: &str, default: bool) -> bool {
    match env::var(name) {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            if value.is_empty() {
                default
            } else {
                !matches!(value.as_str(), "0" | "false" | "no" | "off")
            }
        }
        Err(_) => default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            start_url: Url::parse(DEFAULT_START_URL).unwrap(),
            kiosk: true,
            allowed_hosts: DEFAULT_ALLOWED_HOSTS.iter().map(|h| h.to_string()).collect(),
            allow_all_hosts: false,
            exit_shortcut: DEFAULT_EXIT_SHORTCUT.into(),
            refocus: false,
            incognito: false,
            preflight_media: true,
            preflight_screen: true,
            cover_secondary_displays: true,
            exit_button: true,
            blocked_apps: Vec::new(),
            dev_server: None,
        }
    }

    fn allowed(url: &str) -> bool {
        config().is_navigation_allowed(&Url::parse(url).unwrap())
    }

    #[test]
    fn allows_scaler_and_subdomains() {
        assert!(allowed("https://www.scaler.com/academy"));
        assert!(allowed("https://scaler.com/"));
        assert!(allowed("https://assessments.scaler.com/test/1"));
        assert!(allowed("https://www.interviewbit.com/"));
        assert!(allowed("https://accounts.google.com/o/oauth2/auth"));
    }

    #[test]
    fn allows_google_country_sign_in_domains() {
        assert!(allowed("https://accounts.google.co.in/accounts/SetSID?ssdc=1"));
        assert!(allowed("https://accounts.google.de/accounts/SetSID"));
        assert!(allowed("https://accounts.google.com.br/accounts/SetSID"));
        assert!(!allowed("https://accounts.google.evil.com/"));
        assert!(!allowed("https://accounts.google.co.in.evil.example/"));
        assert!(!allowed("https://www.google.co.in/search?q=answers"));
    }

    #[test]
    fn loadable_is_scheme_based() {
        let ok = |u: &str| Config::is_loadable(&Url::parse(u).unwrap());
        assert!(ok("https://challenges.cloudflare.com/turnstile"));
        assert!(ok("https://www.youtube.com/embed/x"));
        assert!(ok("about:blank"));
        assert!(!ok("file:///etc/passwd"));
        assert!(!ok("ms-settings:privacy"));
    }

    #[test]
    fn allows_bundled_pages() {
        assert!(allowed("tauri://localhost/index.html"));
        assert!(allowed("http://tauri.localhost/index.html"));
        assert!(allowed("about:blank"));
    }

    #[test]
    fn blocks_everything_else() {
        assert!(!allowed("https://www.google.com/search?q=answers"));
        assert!(!allowed("https://notscaler.com/"));
        assert!(!allowed("https://scaler.com.evil.example/"));
        assert!(!allowed("https://evilscaler.com/"));
        assert!(!allowed("file:///etc/passwd"));
        assert!(!allowed("javascript:alert(1)"));
    }

    #[test]
    fn dev_server_origin_is_allowed_only_when_configured() {
        let mut config = config();
        let dev = Url::parse("http://127.0.0.1:1430/index.html").unwrap();
        assert!(!config.is_navigation_allowed(&dev));
        config.allow_dev_server(Some(&Url::parse("http://127.0.0.1:1430").unwrap()));
        assert!(config.is_navigation_allowed(&dev));
        config.allow_dev_server(None);
        assert_eq!(config.allowed_hosts.iter().filter(|h| *h == "127.0.0.1").count(), 1);
        assert!(config.is_bundled_page(&dev));
        assert!(config.is_bundled_page(&Url::parse("tauri://localhost/index.html").unwrap()));
        assert!(config.is_bundled_page(&Url::parse("http://tauri.localhost/cover.html").unwrap()));
        assert!(!config.is_bundled_page(&Url::parse("https://www.scaler.com/").unwrap()));
    }

    #[test]
    fn user_agent_carries_the_kiosk_token() {
        let ua = config().user_agent();
        assert!(ua.ends_with(&format!("{USER_AGENT_TOKEN}/{}", Config::version())));
        assert!(ua.starts_with("Mozilla/5.0"));
    }
}
