// Reload the page once the host is back from a restart (server side: web/power.rs).
//
// A restart notice (Settings → Power, "Boot into generation", data restore) carries
// data-neo-await-boot="<boot id>", the kernel boot id before going down. While such an
// element is in the page, GET /system/boot-id is polled; once a different boot answers,
// the page reloads, so every pane, socket and lock state starts fresh. A stopwatch in
// [data-neo-await-boot-elapsed] shows how long the restart has taken so far. A redirect or
// 401 after the server was gone (edge auth lost the session) reloads too, which then
// leads to the login page.
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) {
    root.NeoRebootWatch = api;
    api.autostart(root);
  }
})(typeof window !== 'undefined' ? window : null, function () {
  'use strict';

  var INTERVAL_MS = 3000;
  var TIMEOUT_MS = 5000;

  /**
   * What one poll means. `res` is { ok, status, type, text } or null (network error /
   * timeout); `sawDown` whether an earlier poll failed.
   * → 'reload' | 'wait'
   */
  function verdict(before, res, sawDown) {
    if (!res) return 'wait';
    if (res.ok) {
      var now = (res.text || '').trim();
      return now && now !== before ? 'reload' : 'wait';
    }
    // Auth redirect (fetch with redirect: 'manual') or 401 once the host was down.
    if (sawDown && (res.type === 'opaqueredirect' || res.status === 401 || res.status === 403)) {
      return 'reload';
    }
    return 'wait';
  }

  /** 75000 → "1:15" */
  function formatElapsed(ms) {
    var total = Math.max(0, Math.floor(ms / 1000));
    var sec = total % 60;
    return Math.floor(total / 60) + ':' + (sec < 10 ? '0' : '') + sec;
  }

  function poll(win) {
    var ctl = typeof win.AbortController === 'function' ? new win.AbortController() : null;
    var timer = ctl ? win.setTimeout(function () { ctl.abort(); }, TIMEOUT_MS) : null;
    return win
      .fetch('/system/boot-id', {
        cache: 'no-store',
        credentials: 'same-origin',
        redirect: 'manual',
        signal: ctl ? ctl.signal : undefined,
      })
      .then(function (r) {
        var base = { ok: r.ok, status: r.status, type: r.type, text: '' };
        if (!r.ok) return base;
        return r.text().then(function (t) {
          base.text = t;
          return base;
        });
      })
      .catch(function () {
        return null;
      })
      .then(function (res) {
        if (timer) win.clearTimeout(timer);
        return res;
      });
  }

  var watching = false;

  function watch(win, el) {
    if (watching) return;
    watching = true;
    var before = el.getAttribute('data-neo-await-boot') || '';
    var sawDown = false;
    var started = Date.now();
    var clock = win.setInterval(function () {
      var cur = win.document.querySelector('[data-neo-await-boot]');
      var out = cur && cur.querySelector('[data-neo-await-boot-elapsed]');
      if (out) out.textContent = formatElapsed(Date.now() - started);
    }, 1000);
    function stop() {
      win.clearInterval(clock);
      watching = false;
    }
    function tick() {
      var cur = win.document.querySelector('[data-neo-await-boot]');
      if (!cur) {
        stop();
        return;
      }
      poll(win).then(function (res) {
        if (!res || !res.ok) sawDown = true;
        if (verdict(before, res, sawDown) === 'reload') {
          var msg = cur.querySelector('[data-neo-await-boot-msg]');
          if (msg) msg.textContent = 'The server is back. Reloading…';
          win.clearInterval(clock);
          win.location.reload();
          return;
        }
        win.setTimeout(tick, INTERVAL_MS);
      });
    }
    win.setTimeout(tick, INTERVAL_MS);
  }

  function scan(win) {
    var el = win.document.querySelector('[data-neo-await-boot]');
    if (el) watch(win, el);
  }

  return {
    verdict: verdict,
    formatElapsed: formatElapsed,
    autostart: function (win) {
      if (!win || !win.document || typeof win.fetch !== 'function') return;
      var doc = win.document;
      // Notices arrive via htmx swaps and Alpine/fetch modals alike.
      if (typeof win.MutationObserver === 'function') {
        var mo = new win.MutationObserver(function () {
          if (!watching) scan(win);
        });
        var start = function () {
          if (doc.body) mo.observe(doc.body, { childList: true, subtree: true });
          scan(win);
        };
        if (doc.body) start();
        else doc.addEventListener('DOMContentLoaded', start);
      }
    },
  };
});
