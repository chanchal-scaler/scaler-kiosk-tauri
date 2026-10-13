# Scaler Kiosk (Tauri)

A [Tauri 2](https://v2.tauri.app) desktop app that runs **scaler.com** inside a locked-down
webview: borderless full-screen kiosk window, hidden Dock/menu bar, no app switching or
force-quit on macOS, a navigation allowlist, and camera / screen-share support (no microphone)
for proctored exams. It is the Tauri counterpart of the Electron shell in
`~/development/scaler-kiosk` — same env variables, same `ScalerKiosk/<version>` User-Agent
token and the same `window.scalerKiosk` global, so the Rails side
(`myinterviewtrainer`) can treat both the same way while you compare the two.

There is no frontend framework: the webview loads a tiny bundled launcher page
(`dist/index.html`) that waits for connectivity, warms up the camera permission
prompts, and then hands the window over to `https://www.scaler.com/`.

## Quick start

Prerequisites: Rust (stable, via rustup), Node 18+, and the platform toolchain Tauri needs —
Xcode Command Line Tools on macOS, Visual Studio Build Tools + the WebView2 runtime on
Windows (see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)).

```bash
cd ~/development/scaler-kiosk-tauri
npm install

# Normal window, no lockdown — the comfortable dev loop
KIOSK=0 npm run dev

# Full kiosk mode (what candidates get). Leave with the Exit button (top-right) or Cmd/Ctrl+Q.
npm run dev

# Point it at a local Rails server instead of production
KIOSK=0 APP_URL=http://localhost:3000 npm run dev

# Release bundle (.app/.dmg on macOS, .msi/.exe on Windows) in src-tauri/target/release/bundle
npm run build
```

The first `npm run dev` compiles the Rust side (a few minutes); later runs are incremental.
Under `tauri dev` the bundled pages are served from Tauri's dev server (`http://127.0.0.1:1430`)
rather than `tauri://localhost`; that origin is allowlisted automatically in dev builds only.
Replace the placeholder icon with the real logo (1024×1024 PNG) and regenerate the icon set:

```bash
cp /path/to/scaler-logo.png app-icon.png && npm run icons
```

## What kiosk mode does

| Behaviour | Where |
| --- | --- |
| Borderless window the size of the primary display, always on top, visible on every Space, not resizable/minimizable/closable | `src-tauri/src/kiosk.rs` |
| macOS: Dock and menu bar hidden; Cmd+Tab, force quit (Cmd+Opt+Esc), Hide, log out / shut down disabled (`NSApplication.presentationOptions`, the mechanism Apple's kiosk guidance and SEB use) | `src-tauri/src/platform/macos.rs` |
| macOS: menu without Hide/Minimize/Close (so Cmd+H, Cmd+M, Cmd+W do nothing), a Quit item that asks first, and the Edit items so copy/paste keeps working | `src-tauri/src/platform/macos.rs` |
| Windows/Linux: borderless fullscreen, always on top, hidden from the taskbar | `src-tauri/src/kiosk.rs` |
| Every other display is covered by a black window | `kiosk.rs` + `dist/cover.html` |
| The main window may only show `scaler.com`, `interviewbit.com` and Google sign-in pages (including country domains like `accounts.google.co.in`) plus the start URL's host; if it lands anywhere else it is sent back to the last allowed page. Iframes (Cloudflare Turnstile, YouTube embeds) are not restricted, only `file:` and custom schemes are blocked everywhere | `src-tauri/src/config.rs`, `guard_page_load` in `kiosk.rs` |
| `window.open()` / `target="_blank"` never open a second window — the URL loads in the kiosk window if allowed | `kiosk.rs` + `src-tauri/scripts/kiosk.js` |
| `scaler-kiosk://` deep links open the kiosk directly on a test page (see below) | `src-tauri/src/deeplink.rs` |
| Context menu and zoom shortcuts disabled | `scripts/kiosk.js`, `kiosk.rs` |
| Nothing closes the kiosk without an "Exit kiosk?" confirmation (see [Quitting](#quitting)), except a page calling `exit_kiosk` after the exam | `src-tauri/src/lib.rs`, `kiosk.rs` |
| Programs in `KIOSK_BLOCKED_APPS` (default: Claude desktop app and Claude Code CLI) are killed once at start-up (`pkill` on macOS/Linux, `taskkill` on Windows) | `kill_blocked_apps` in `kiosk.rs` |

Lockdown is deliberately *not* perfect: trackpad gestures for Mission Control, hot corners,
and the power button are OS features outside the app. Blocked apps are only killed at
start-up, so one relaunched during the exam keeps running.

## Quitting

In kiosk mode every one of these shows the same native **"Exit kiosk?"** dialog (Exit kiosk /
Cancel); nothing quits without confirming:

| Way out | Where it works |
| --- | --- |
| **Exit button**: a small pill in the top-right corner, half-transparent until hovered | every page (turn off with `KIOSK_EXIT_BUTTON=0`) |
| **Cmd+Q** | macOS (a menu item, so it works even though the menu bar is hidden) |
| **Ctrl+Q** | Windows, Linux |
| **Alt+F4** | Windows, Linux |
| **Control+Alt+Shift+Q** | everywhere, system-wide, even if the page is frozen (not under Linux Wayland) |

Pages can also quit without a dialog by calling `window.scalerKiosk.exit()`, e.g. once the
exam is submitted. The button is injected into the page in a closed shadow root attached to
`<html>`, so site CSS can't restyle it and single-page-app re-renders can't remove it. If it
covers something on the exam pages, change `top`/`right` in `src-tauri/scripts/kiosk.js`.

## Camera and screen share (no microphone)

* **Microphone** — not used. Microphone requests are always denied (`on_permission_request`
  in `kiosk.rs`) and there is no microphone usage string or entitlement. Exam pages must ask
  for video only: `getUserMedia({ video: true, audio: true })` fails as a whole with
  `NotAllowedError`, because WebKit treats camera+mic as one request.
* **Camera** — `getUserMedia({ video: true })` is granted at the webview level without a
  per-site prompt. The OS still asks once per machine. macOS needs the usage string in
  `src-tauri/Info.plist` (Tauri merges it into the bundle and embeds it in the dev
  binary) and, for signed/notarized builds with the Hardened Runtime, the camera entitlement
  in `src-tauri/Entitlements.plist`.
* **Screen share** — `getDisplayMedia()` works without extra code on **Windows** (WebView2
  shows its own picker) and on **macOS with the WebKit shipped in macOS 14.6+/Safari
  17.6+**, where the system "share your screen" picker appears and macOS asks for *Screen
  Recording* permission on first use. Older WebKit builds (macOS 14.0–14.5 without the
  Safari 17.6 update, macOS 13) deny `getDisplayMedia()` with `NotAllowedError` whenever the
  embedding app implements the camera/mic delegate, which wry does — see WebKit bugs
  [274896](https://bugs.webkit.org/show_bug.cgi?id=274896) (fixed May 2024) and
  [271688](https://bugs.webkit.org/show_bug.cgi?id=271688). The known workaround for those
  versions is implementing WebKit's private
  `_webView:requestDisplayCapturePermissionForOrigin:…` delegate method, which this
  scaffold intentionally does not do. Unlike the Electron shell, there is no way to capture
  the screen silently without a picker: WKWebView always goes through the system picker.
* The launcher page triggers the camera prompt before the exam starts
  (`KIOSK_PREFLIGHT_MEDIA`), and on macOS the app triggers the Screen Recording prompt at
  start-up (`KIOSK_PREFLIGHT_SCREEN`), because macOS may require a relaunch after that
  permission is granted — better before the exam than during it.
* During `npm run dev` macOS attributes permissions to the process that launched the app
  (Terminal, iTerm, VS Code), so test permission flows with a bundled build too.

## Windows and Linux

Tauri builds natively on each OS. macOS is built on your Mac; Windows and Linux are built
by GitHub Actions (`.github/workflows/build.yml`), or by hand on those OSes as described below.

### Releases: macOS locally, Windows + Linux on GitHub Actions

```bash
# macOS (Apple Silicon + Intel in one app), on your Mac
npm run build:mac
# → src-tauri/target/universal-apple-darwin/release/bundle/dmg/*.dmg

# Windows + Linux: push to GitHub
git push                          # main / PRs: installers uploaded as run artifacts
git tag v0.1.0 && git push --tags # tag: also creates a *draft* release with them attached
```

* **Every push to `main`, every PR and manual runs** (Actions → "Build Windows & Linux" →
  Run workflow) build the Windows `.msi` + `-setup.exe` (on `windows-latest`) and the Linux
  `.deb`, `.rpm` and `.AppImage` (on `ubuntu-22.04`, so they also run on older distros).
  Download them from the run page under **Artifacts** (kept 14 days).
* **A `v*` tag** also creates a draft GitHub release with those installers attached. Drag
  the macOS `.dmg` from `npm run build:mac` into the same draft, then publish it. Keep the
  tag in step with `version` in `tauri.conf.json`/`package.json`, which is what ends up in
  the file names.
* Cost on a private repo: roughly 10 min Windows (billed at ~1.7× Linux) plus 8 min Linux
  per run once the Rust cache is warm, i.e. about 25 of the 2,000 free monthly minutes. The
  first run, with a cold cache, takes longer.
* Code signing is not wired in. Windows: add the certificate as a repo secret and set
  `bundle.windows.certificateThumbprint` / `signCommand` (see Tauri's Windows signing guide)
  before handing installers to candidates, or SmartScreen will warn.

### Windows 10/11

1. Install [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
   with the **Desktop development with C++** workload.
2. Install Rust with [rustup-init.exe](https://rustup.rs) (default MSVC toolchain) and Node 18+.
3. WebView2 is preinstalled on Windows 10 (1803+) and 11; the installer bundles the
   bootstrapper (`webviewInstallMode: embedBootstrapper`) for machines that lack it.

```powershell
cd scaler-kiosk-tauri
npm install
$env:KIOSK="0"; npm run dev        # windowed (PowerShell; in cmd: set KIOSK=0 && npm run dev)
Remove-Item Env:KIOSK; npm run dev # full kiosk; exit with the Exit button or Ctrl+Q
npm run build                      # src-tauri\target\release\bundle\msi\*.msi and nsis\*-setup.exe
```

The installers register the `scaler-kiosk://` scheme; in dev the binary registers it for
itself on first run. Kiosk logs: `%LOCALAPPDATA%\com.scaler.kiosk.tauri\logs\kiosk.log`.

Limits on Windows: the app is a borderless, always-on-top fullscreen window, but Alt+Tab,
the Windows key, Ctrl+Alt+Del and Task Manager are not blocked (that needs Assigned Access /
a kiosk policy, or a low-level keyboard hook), and WebView2's own shortcuts (F5 / Ctrl+R
reload, Ctrl+P print, Ctrl+F find) still work. Sign the installer or SmartScreen will warn.

### Linux (Debian/Ubuntu shown)

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev \
  gstreamer1.0-plugins-good gstreamer1.0-plugins-bad   # camera capture at runtime
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # Rust; plus Node 18+

cd scaler-kiosk-tauri
npm install
KIOSK=0 npm run dev
npm run build      # .deb, .rpm and .AppImage under src-tauri/target/release/bundle/
```

Fedora: `sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file
libappindicator-gtk3-devel librsvg2-devel libxdo-devel` and `sudo dnf group install
c-development`. Other distros: see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

Linux specifics:

* WebKitGTK ships with getUserMedia/WebRTC off; the app turns them on
  (`configure_webview` in `platform/other.rs`), so the camera works.
* Screen share goes through the desktop's xdg-desktop-portal + PipeWire picker (standard on
  current GNOME/KDE); it needs a recent WebKitGTK (Ubuntu 24.04's is fine).
* The global backup shortcut (Ctrl+Alt+Shift+Q) only works in an **X11** session; under
  Wayland it fails to register (logged). The Exit button, Ctrl+Q and Alt+F4 work either way.
* No app-level lockdown beyond fullscreen + always-on-top; a real Linux kiosk is a
  dedicated session (e.g. GNOME Kiosk / a cage compositor) that only launches this app.
* Logs: `~/.local/share/com.scaler.kiosk.tauri/logs/kiosk.log`.

## Configuration

Compiled-in defaults live in `src-tauri/src/config.rs`; environment variables override them
(only practical for development — a bundled app started from Finder/Explorer sees none, so
bake production values into `config.rs`).

| Variable | Default | Meaning |
| --- | --- | --- |
| `KIOSK` | `1` | `0` runs a normal 1280×800 window with no lockdown |
| `APP_URL` | `https://www.scaler.com/` | first page to open; its host is auto-allowlisted |
| `ALLOWED_HOSTS` | – | extra comma-separated hosts (subdomains included); `*` disables the allowlist |
| `EXIT_SHORTCUT` | `Control+Alt+Shift+Q` | system-wide backup quit combination (asks for confirmation), works even if the page hangs; empty disables it |
| `KIOSK_EXIT_BUTTON` | `1` | show the Exit button in the top-right corner (kiosk mode only) |
| `KIOSK_BLOCKED_APPS` | `claude` | comma-separated program names killed at start-up, matched case-insensitively (kiosk mode only); empty disables it |
| `KIOSK_REFOCUS` | `0` | pull focus back 1.5 s after another window steals it (can fight with OS permission dialogs and the share picker, hence off) |
| `KIOSK_INCOGNITO` | `0` | ephemeral cookies/storage |
| `KIOSK_PREFLIGHT_MEDIA` | `1` | launcher asks for the camera before opening scaler.com |
| `KIOSK_PREFLIGHT_SCREEN` | `=KIOSK` | macOS: trigger the Screen Recording prompt at start-up |
| `KIOSK_COVER_DISPLAYS` | `=KIOSK` | black out secondary displays |

Logs go to stdout and to the platform log directory
(`~/Library/Logs/com.scaler.kiosk.tauri/kiosk.log` on macOS,
`%LOCALAPPDATA%\com.scaler.kiosk.tauri\logs` on Windows). Blocked navigations, popups and
page loads are logged.

## How scaler.com can detect and drive the kiosk

* **Server side:** the User-Agent ends with `ScalerKiosk/<version>` (the rest looks like
  Safari/Edge so sign-in providers don't reject the webview). The Rails app currently
  recognises SEB via `SEB_USER_AGENT` in `app/controllers/seb_configs_controller.rb`; a
  `/ScalerKiosk\//` check alongside it is the equivalent. Treat it as a hint, not proof —
  anyone can set a User-Agent. Note that WKWebView cannot add custom headers to every
  request, so the Electron shell's `X-Scaler-Kiosk-Hash` header has no Tauri equivalent;
  attestation here would go through an IPC command instead (see next steps).
* **Client side:** every page gets `window.scalerKiosk` injected before its own scripts run.
  `version` and `platform` match the Electron shell; the methods are Tauri-only:

  ```js
  if (window.scalerKiosk) {
    const info = await window.scalerKiosk.info(); // { name, version, platform, kiosk, startUrl, allowedHosts }
    window.scalerKiosk.log('info', 'exam started');
    // after the exam is submitted (quits straight away, no confirmation):
    await window.scalerKiosk.exit();
    // or offer your own "Leave" button that shows the native confirmation:
    window.scalerKiosk.requestExit();
  }
  ```

  `info`, `log` and `exit` work only on pages served from `https://scaler.com` /
  `https://*.scaler.com` (`src-tauri/capabilities/scaler.json`). `requestExit` works on every
  page (`exit-anywhere.json`), since it only asks the user. Nothing else from the Tauri API
  is exposed to remote content. Add hosts to `scaler.json` if the exam pages live elsewhere.

## Deep links (open the kiosk on a specific test)

The app registers the `scaler-kiosk://` URL scheme (`plugins.deep-link` in
`tauri.conf.json`). Any scaler.com URL becomes a kiosk link by swapping the scheme — the same
trick as SEB's `sebs://` links — and there is an explicit form too:

```
scaler-kiosk://www.scaler.com/test/abc123
scaler-kiosk://open?url=https%3A%2F%2Fwww.scaler.com%2Ftest%2Fabc123
```

* Cold start: the launcher opens the linked page instead of the home page. Already running:
  the kiosk window navigates to it and comes to the front (a second process on Windows/Linux
  hands the link over and exits, via `tauri-plugin-single-instance`).
* Only pages that pass the navigation allowlist are honoured; anything else is logged and
  ignored, so a link can't take the kiosk off-site.
* Rails side: generate these links the way `generateSebLink` produces `seb://` links, or just
  replace `https://` with `scaler-kiosk://` in the normal test URL. The page itself can still
  check `window.scalerKiosk` / the User-Agent to confirm it really is inside the kiosk.
* **Testing on macOS needs a bundled app** — the scheme is registered through the `.app`'s
  Info.plist, so `tauri dev` can't receive links. Run `npm run build`, open
  `src-tauri/target/release/bundle/macos/Scaler Kiosk.app` once (that registers it with
  Launch Services), then `open "scaler-kiosk://www.scaler.com/test/abc123"`. On Windows
  the installer registers the scheme (the dev binary registers itself on first run). On
  Linux the app registers a per-user handler every time it starts, because the packaged
  `.desktop` file doesn't pass the URL on (no `%u`); after one launch,
  `xdg-open "scaler-kiosk://www.scaler.com/test/abc123"` works. In dev on any OS you
  can also pass a page URL as an argument: `npm run dev -- -- https://www.scaler.com/test/abc123`.
* Universal links (plain `https://scaler.com/...` URLs opening the app) are not set up: on
  macOS they need an Apple Developer team with the associated-domains entitlement plus an
  `apple-app-site-association` file on scaler.com, and on Windows a web-to-app link manifest.
  The custom scheme is what SEB uses and needs none of that.

## Project layout

```
scaler-kiosk-tauri/
├── .github/workflows/build.yml  # CI: Windows + Linux installers, draft release on v* tags
├── package.json                 # @tauri-apps/cli only; `npm run dev|build|build:mac|icons`
├── app-icon.png                 # placeholder source icon — replace, then `npm run icons`
├── dist/                        # bundled pages served as tauri://localhost
│   ├── index.html, launcher.js  #   launcher: connectivity wait, media preflight, redirect
│   └── cover.html               #   black page for secondary displays
└── src-tauri/
    ├── tauri.conf.json          # id com.scaler.kiosk.tauri, CSP for bundled pages, deep-link scheme, bundle settings
    ├── Cargo.toml, build.rs     # deps; build.rs declares the IPC commands for the permission system
    ├── Info.plist               # NSCameraUsageDescription (merged into the bundle)
    ├── Entitlements.plist       # hardened-runtime camera entitlement
    ├── capabilities/
    │   ├── launcher.json        #   local launcher page: core API + kiosk commands
    │   ├── scaler.json          #   remote scaler.com pages: kiosk commands only
    │   └── exit-anywhere.json   #   any page: request_exit (Exit button / Ctrl+Q)
    ├── permissions/autogenerated/  # generated by build.rs from the command list
    ├── scripts/kiosk.js         # init script injected into every page (window.scalerKiosk)
    ├── icons/                   # generated icon set
    └── src/
        ├── main.rs, lib.rs      # app builder, close/exit guards
        ├── config.rs            # defaults + env overrides + navigation allowlist (unit tested)
        ├── kiosk.rs             # window creation, popups, permissions, exit shortcut, display covers
        ├── commands.rs          # exit_kiosk, kiosk_info, kiosk_log
        ├── deeplink.rs          # scaler-kiosk:// links → allowed page (launch + while running)
        └── platform/            # macos.rs (presentation options, menu, screen-recording preflight), other.rs
```

## Signing and distribution (macOS)

`npm run build` produces an unsigned `.app`/`.dmg`, which is fine for local testing. For
candidates' machines sign and notarize: set `APPLE_SIGNING_IDENTITY`, `APPLE_ID`,
`APPLE_PASSWORD` and `APPLE_TEAM_ID` (see
[Tauri's macOS code signing guide](https://v2.tauri.app/distribute/sign/macos/)). The
hardened runtime is on by default, which is why `Entitlements.plist` exists. Windows builds
should be signed too, or SmartScreen will warn.

## Not in this scaffold (next steps)

* Background-app detection/killing and process monitoring.
* Cryptographic attestation (a signed per-session token returned by `kiosk_info`, the
  Tauri stand-in for the SEB config-key hash / the Electron shell's request header).
* A passcode or server-approved exit instead of a confirmed key combination.
* Windows OS-level lockdown (Assigned Access) and a Linux kiosk session.
* `getDisplayMedia()` on macOS builds older than 14.6 (private WebKit delegate).
* Auto-update (`tauri-plugin-updater`) and crash reporting.
