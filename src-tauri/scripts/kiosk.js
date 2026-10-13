// Injected into every page the kiosk loads (launcher and scaler.com) before any page
// script runs. `__KIOSK_CONFIG__` is replaced with a JSON object by src/kiosk.rs.
//
// Pages can detect the kiosk and talk to it through `window.scalerKiosk` (same name and
// `{ version, platform }` fields as the Electron scaffold, plus a few methods):
//   window.scalerKiosk.version            -> "0.1.0"
//   window.scalerKiosk.info()             -> Promise<{ name, version, platform, kiosk, startUrl, allowedHosts }>
//   window.scalerKiosk.exit()             -> quits the kiosk right away (call it after the exam is submitted)
//   window.scalerKiosk.requestExit()      -> shows the "Exit kiosk?" confirmation, like the Exit button
//   window.scalerKiosk.log(level, msg)    -> writes to the kiosk log file
// The server side can detect it from the User-Agent, which ends with "ScalerKiosk/<version>".
(function () {
  'use strict';
  // Top-level page only: WebView2 (Windows) also runs initialization scripts in iframes.
  if (window.self !== window.top) return;
  if (window.scalerKiosk) return;

  var config = __KIOSK_CONFIG__;

  function invoke(cmd, args) {
    var internals = window.__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== 'function') {
      return Promise.reject(new Error('Scaler Kiosk bridge is not available on this page'));
    }
    return internals.invoke(cmd, args || {});
  }

  function requestExit() {
    return invoke('request_exit').catch(function (err) {
      console.warn('[scaler-kiosk] exit request failed:', err);
    });
  }

  var api = Object.freeze({
    name: 'ScalerKiosk',
    version: config.version,
    platform: config.platform,
    kiosk: config.kiosk,
    startUrl: config.startUrl,
    preflightMedia: config.preflightMedia,
    info: function () { return invoke('kiosk_info'); },
    exit: function () { return invoke('exit_kiosk'); },
    requestExit: requestExit,
    log: function (level, message) {
      return invoke('kiosk_log', { level: String(level), message: String(message) }).catch(function () {});
    }
  });

  Object.defineProperty(window, 'scalerKiosk', { value: api, writable: false, configurable: false });

  if (!config.kiosk) return;

  // Popups never leave the kiosk: load them in this window instead (the navigation
  // allowlist in src/config.rs still applies).
  window.open = function (url) {
    if (url) window.location.assign(String(url));
    return null;
  };
  document.addEventListener('click', function (event) {
    var target = event.target;
    var anchor = target && target.closest ? target.closest('a[target]') : null;
    if (anchor && /^_blank$/i.test(anchor.getAttribute('target') || '')) {
      anchor.setAttribute('target', '_self');
    }
  }, true);

  // No context menu in kiosk mode.
  document.addEventListener('contextmenu', function (event) { event.preventDefault(); }, true);

  // Ctrl+Q (Windows/Linux) asks to quit. On macOS Cmd+Q is handled by the app menu before
  // the page sees it, so this only matters elsewhere. Registered before any page script,
  // in the capture phase, so the page can't swallow it.
  window.addEventListener('keydown', function (event) {
    var key = (event.key || '').toLowerCase();
    if (key === 'q' && (event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey) {
      event.preventDefault();
      event.stopPropagation();
      requestExit();
    }
  }, true);

  if (!config.exitButton) return;

  // Exit button: a small pill pinned to the top-right corner, half transparent until
  // hovered. It lives in a closed shadow root attached to <html> (not <body>), so page CSS
  // can't restyle it and SPA re-renders of <body> can't remove it. Styles are set through
  // the CSSOM, which strict page CSPs allow (unlike inline <style> elements).
  var host = null;

  function buildButton() {
    host = document.createElement('div');
    host.setAttribute('data-scaler-kiosk', 'exit');
    var s = host.style;
    s.position = 'fixed';
    s.top = '10px';
    s.right = '10px';
    s.zIndex = '2147483647';
    s.margin = '0';
    s.padding = '0';

    var shadow = host.attachShadow({ mode: 'closed' });
    var button = document.createElement('button');
    button.type = 'button';
    button.title = 'Exit Scaler Kiosk';
    button.setAttribute('aria-label', 'Exit Scaler Kiosk');
    button.textContent = '✕  Exit';
    var b = button.style;
    b.font = '600 12px/1 -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif';
    b.color = '#ffffff';
    b.background = 'rgba(17, 24, 39, 0.78)';
    b.border = '1px solid rgba(255, 255, 255, 0.25)';
    b.borderRadius = '999px';
    b.padding = '7px 12px';
    b.cursor = 'pointer';
    b.opacity = '0.55';
    b.transition = 'opacity 120ms ease';
    b.boxShadow = '0 2px 8px rgba(0, 0, 0, 0.25)';
    b.whiteSpace = 'nowrap';
    button.addEventListener('mouseenter', function () { b.opacity = '1'; });
    button.addEventListener('mouseleave', function () { b.opacity = '0.55'; });
    button.addEventListener('focus', function () { b.opacity = '1'; });
    button.addEventListener('blur', function () { b.opacity = '0.55'; });
    button.addEventListener('click', function (event) {
      event.preventDefault();
      event.stopPropagation();
      requestExit();
    });
    shadow.appendChild(button);
  }

  function mount() {
    var root = document.documentElement;
    if (!root) return;
    if (!host) buildButton();
    if (host.parentNode !== root || root.lastElementChild !== host) {
      root.appendChild(host); // (re)attach, and stay last so it paints on top
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', mount);
  } else {
    mount();
  }
  // Cheap safety net for pages that rewrite <html> or append full-screen overlays later.
  setInterval(mount, 2000);
})();
