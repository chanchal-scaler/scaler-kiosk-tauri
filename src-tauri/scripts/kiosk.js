// Injected into every page the kiosk loads (launcher and scaler.com) before any page
// script runs. `__KIOSK_CONFIG__` is replaced with a JSON object by src/kiosk.rs.
//
// Pages can detect the kiosk and talk to it through `window.scalerKiosk` (same name and
// `{ version, platform }` fields as the Electron scaffold, plus a few methods):
//   window.scalerKiosk.version            -> "0.1.0"
//   window.scalerKiosk.info()             -> Promise<{ name, version, platform, kiosk, startUrl, allowedHosts }>
//   window.scalerKiosk.exit()             -> quits the kiosk (call it after the exam is submitted)
//   window.scalerKiosk.log(level, msg)    -> writes to the kiosk log file
// The server side can detect it from the User-Agent, which ends with "ScalerKiosk/<version>".
(function () {
  'use strict';
  if (window.scalerKiosk) return;

  var config = __KIOSK_CONFIG__;

  function invoke(cmd, args) {
    var internals = window.__TAURI_INTERNALS__;
    if (!internals || typeof internals.invoke !== 'function') {
      return Promise.reject(new Error('Scaler Kiosk bridge is not available on this page'));
    }
    return internals.invoke(cmd, args || {});
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
})();
