// Launcher page: waits for connectivity, warms up the camera permission
// prompts, then hands the window over to scaler.com. The kiosk shell injects
// `window.scalerKiosk` (see src-tauri/scripts/kiosk.js) with the start URL.
(function () {
  'use strict';

  var kiosk = window.scalerKiosk || {};
  var startUrl = kiosk.startUrl || 'https://www.scaler.com/';
  var preflightMedia = kiosk.preflightMedia !== false;

  function el(id) { return document.getElementById(id); }
  function setStatus(text, cls) {
    var node = el('status');
    node.textContent = text;
    node.className = cls || 'muted';
  }
  function setHint(text) { el('hint').textContent = text || ''; }
  function sleep(ms) { return new Promise(function (resolve) { setTimeout(resolve, ms); }); }
  function logToShell(level, message) {
    if (kiosk.log) { kiosk.log(level, message); } else { console.log('[kiosk]', level, message); }
  }

  // An opaque (no-cors) response is enough to prove the host is reachable.
  function online() {
    return fetch(startUrl, { mode: 'no-cors', cache: 'no-store', credentials: 'omit' })
      .then(function () { return true; })
      .catch(function () { return false; });
  }

  // Trigger the OS camera prompt before the exam page needs it, so the
  // candidate deals with the system dialogs here rather than mid-exam.
  function warmUpMedia() {
    if (!preflightMedia || !navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
      return Promise.resolve();
    }
    setStatus('Requesting camera access…');
    setHint('If macOS or Windows asks, choose "Allow".');
    return navigator.mediaDevices.getUserMedia({ video: true, audio: false })
      .then(function (stream) {
        stream.getTracks().forEach(function (track) { track.stop(); });
        logToShell('info', 'media preflight ok');
      })
      .catch(function (err) {
        // Not fatal here: the exam page asks again and shows its own guidance.
        logToShell('warn', 'media preflight failed: ' + (err && err.name) + ' ' + (err && err.message));
      })
      .then(function () { setHint(''); });
  }

  async function main() {
    var attempt = 0;
    while (!(await online())) {
      attempt += 1;
      setStatus('No internet connection. Retrying… (' + attempt + ')', 'error');
      setHint('Connect to Wi‑Fi or Ethernet; the exam will open automatically.');
      await sleep(Math.min(2000 * attempt, 10000));
    }
    setStatus('Connected.');
    await warmUpMedia();
    setStatus('Opening Scaler…');
    // A deep link (scaler-kiosk://…) may have arrived while this page was up: ask the shell
    // for the current target rather than trusting the value injected at start-up.
    if (kiosk.info) {
      try {
        var info = await kiosk.info();
        if (info && info.startUrl) startUrl = info.startUrl;
      } catch (err) {
        logToShell('warn', 'could not refresh start URL: ' + err);
      }
    }
    logToShell('info', 'launcher navigating to ' + startUrl);
    window.location.replace(startUrl);
  }

  main();
})();
