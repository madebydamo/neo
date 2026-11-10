// Service health: one shared summarizer + page-wide status dots.
//
// NeoUnitHealth.summarize(entries) folds per-unit health (server-derived from
// systemd properties, else ActiveState) into one service state. The option pane (unit_watch.js unitSummary) and the services grid /
// navigator dots both call it, so they always agree.
//
// Dots: any element with data-service-status="<name>" data-units="a b"
// data-timers="t". After render (never blocking it), every dot on the page is
// refreshed with ONE POST /status/services; the server answers from a single
// batched `systemctl show` behind a few-second single-flight cache. Polls every
// 15s while the tab is visible; pauses while hidden.
(function (root, factory) {
  var api = factory((root && root.NeoUtil) || require('./neo_util.js'));
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) {
    root.NeoUnitHealth = api.health;
    root.NeoServiceStatus = api.dots;
    api.dots.autostart(root);
  }
})(typeof window !== 'undefined' ? window : null, function (NeoUtil) {
  'use strict';

  var BUSY = { activating: 1, deactivating: 1, reloading: 1, refreshing: 1 };

  /**
   * Per-unit health. The server derives it from systemd properties (Type,
   * RemainAfterExit, SubState, Result; units/status.rs UnitHealth); without it
   * (older callers / tests) fall back to ActiveState + LoadState.
   * @returns {?string} running | done | setup | changing | idle | stopped |
   *   failed | missing | unknown, or null while pending
   */
  function healthOf(e) {
    if (!e.state && !e.health) return null;
    if (e.health) return e.health;
    var st = e.state;
    if (e.load === 'not-found' && st !== 'failed') return 'missing';
    if (st === 'active') return 'running';
    if (st === 'failed') return 'failed';
    if (BUSY[st]) return 'changing';
    if (st === 'unknown') return 'unknown';
    return 'stopped';
  }

  /**
   * @param {Array<{state: ?string, health?: string, timer?: boolean, load?: string}>} entries
   *   state:  ActiveState, or null/'' while not known yet (pane waiting on WS).
   *   health: server-derived unit health (see healthOf); preferred over state.
   *   load:   LoadState when known ('not-found' = unit not deployed yet).
   * @returns {{state: string, text: string}}
   *   state ∈ ok | partial | changing | failed | down | none | idle | unknown
   *   A finished setup unit ('done') counts as up, so "1/2 running" only means
   *   a daemon is really down. No units at all is 'idle' (configuration only).
   */
  function summarize(entries) {
    entries = entries || [];
    if (!entries.length) return { state: 'idle', text: 'configured · no processes' };
    var total = 0, up = 0, failed = 0, busy = 0, setup = 0, unknown = 0, missing = 0, pending = 0;
    for (var i = 0; i < entries.length; i++) {
      var e = entries[i] || {};
      var h = healthOf(e);
      if (h === null) { pending++; continue; }
      // Timer-backed oneshots idle between runs; only a failure counts against them.
      if (e.timer && h !== 'failed') continue;
      // A oneshot job that never ran waits for its trigger: neither up nor down.
      if (h === 'idle') continue;
      total++;
      if (h === 'missing') missing++;
      else if (h === 'running' || h === 'done') up++;
      else if (h === 'failed') failed++;
      else if (h === 'setup') { busy++; setup++; }
      else if (h === 'changing') busy++;
      else if (h === 'unknown') unknown++;
    }
    if (pending) return { state: 'unknown', text: 'checking…' };
    if (failed) return { state: 'failed', text: failed === 1 ? '1 failed' : failed + ' failed' };
    if (total && missing === total) return { state: 'none', text: 'not deployed' };
    if (unknown) return { state: 'none', text: 'status unknown' };
    if (busy) return { state: 'changing', text: setup === busy ? 'setting up…' : 'changing…' };
    if (total === 0) return { state: 'ok', text: 'idle' };
    if (up === total) return { state: 'ok', text: 'running' };
    if (up === 0) return { state: 'down', text: 'stopped' };
    return { state: 'partial', text: up + '/' + total + ' running' };
  }

  function words(s) {
    return String(s || '').split(/\s+/).filter(Boolean);
  }

  /** Request body for every dot under `scope` (deduped by service name). */
  function collectRequest(scope) {
    var els = scope.querySelectorAll('[data-service-status]');
    var services = {};
    var n = 0;
    for (var i = 0; i < els.length; i++) {
      var name = els[i].getAttribute('data-service-status');
      if (!name || services[name]) continue;
      services[name] = {
        units: words(els[i].getAttribute('data-units')),
        timers: words(els[i].getAttribute('data-timers')),
      };
      n++;
    }
    return { count: n, body: { services: services } };
  }

  /** Server response row → summarize() input. */
  function entriesFor(resp, name) {
    var svc = resp && resp.services && resp.services[name];
    if (!svc || !svc.units) return null;
    return svc.units.map(function (u) {
      return { state: u.active || 'unknown', health: u.health, timer: !!u.timer, load: u.load };
    });
  }

  function paint(el, summary) {
    var name = el.getAttribute('data-service-status');
    el.setAttribute('data-state', summary.state);
    el.setAttribute('title', name + ': ' + summary.text);
    el.setAttribute('aria-label', name + ': ' + summary.text);
  }

  /** Apply one response to every dot under `scope`. */
  function apply(scope, resp) {
    var els = scope.querySelectorAll('[data-service-status]');
    for (var i = 0; i < els.length; i++) {
      var entries = entriesFor(resp, els[i].getAttribute('data-service-status'));
      paint(els[i], entries ? summarize(entries) : { state: 'none', text: 'status unknown' });
    }
  }

  /**
   * Paint unpainted dots under `scope` from an earlier response, where it covered
   * exactly the dot's units (a re-rendered grid shows the last state, not "checking",
   * until the next request answers). Returns how many dots are still unpainted.
   */
  function paintFresh(scope, resp) {
    var els = scope.querySelectorAll('[data-service-status]:not([data-state])');
    var left = 0;
    for (var i = 0; i < els.length; i++) {
      var svc = resp && resp.services && resp.services[els[i].getAttribute('data-service-status')];
      var units = words(els[i].getAttribute('data-units'));
      var known = svc && svc.units && svc.units.map(function (u) { return u.name; });
      if (known && known.length === units.length && units.every(function (u) { return known.indexOf(u) >= 0; })) {
        paint(els[i], summarize(entriesFor(resp, els[i].getAttribute('data-service-status'))));
      } else {
        left++;
      }
    }
    return left;
  }

  var POLL_MS = 15000;

  var dots = {
    POLL_MS: POLL_MS,
    collectRequest: collectRequest,
    apply: apply,
    paintFresh: paintFresh,
    _timer: null,
    _inflight: false,
    _again: false,
    _win: null,
    /** Last successful response (paints re-rendered dots at once). */
    _last: null,

    /** One batched request for all dots on the page (coalesces overlapping calls). */
    refresh: function () {
      var win = dots._win;
      if (!win || !win.document) return;
      var doc = win.document;
      var req = collectRequest(doc);
      if (!req.count) return;
      if (dots._inflight) { dots._again = true; return; }
      dots._inflight = true;
      var done = function () {
        dots._inflight = false;
        if (dots._again) { dots._again = false; dots.refresh(); }
      };
      win.fetch('/status/services', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/json' },
        credentials: 'same-origin',
        body: JSON.stringify(req.body),
      })
        .then(function (r) { return r.ok ? r.json() : null; })
        .then(function (resp) {
          dots._last = resp;
          apply(doc, resp || {});
        })
        .catch(function () { apply(doc, {}); })
        .then(done, done);
    },

    _schedule: function () {
      var win = dots._win;
      if (dots._timer) { win.clearTimeout(dots._timer); dots._timer = null; }
      if (win.document.visibilityState === 'hidden') return;
      if (!win.document.querySelector('[data-service-status]')) return;
      dots._timer = win.setTimeout(function () {
        dots._timer = null;
        dots.refresh();
        dots._schedule();
      }, POLL_MS);
    },

    /** Refresh now and restart the poll clock. */
    kick: function () {
      dots.refresh();
      dots._schedule();
    },

    autostart: function (win) {
      if (!win || !win.document || typeof win.fetch !== 'function') return;
      dots._win = win;
      var doc = win.document;
      var start = function () {
        // Next tick: never compete with first paint.
        win.setTimeout(dots.kick, 0);
      };
      NeoUtil.onReady(doc, start);

      doc.addEventListener('visibilitychange', function () {
        if (doc.visibilityState === 'visible') dots.kick();
        else dots._schedule(); // clears the timer while hidden
      });

      // htmx swaps (grid partial, nav-services, a newer evaluation's grid) may bring
      // new dots: paint them from the last response, then fetch once for them. The
      // whole page is checked: after an outerHTML swap detail.target is the replaced
      // (detached, already painted) element, not the new one.
      var pendingSwap = null;
      doc.addEventListener('htmx:afterSettle', function () {
        if (!doc.querySelector('[data-service-status]:not([data-state])')) return;
        paintFresh(doc, dots._last);
        if (pendingSwap) win.clearTimeout(pendingSwap);
        pendingSwap = win.setTimeout(function () { pendingSwap = null; dots.kick(); }, 50);
      });
    },
  };

  return { health: { summarize: summarize, healthOf: healthOf }, dots: dots };
});
