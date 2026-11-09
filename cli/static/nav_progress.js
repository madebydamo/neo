/*
 * Neo navigation loading feedback (generic, htmx-event driven).
 *
 * For every tracked in-flight request:
 *   - immediately marks the clicked element (`data-neo-loading`, aria-busy) so the
 *     user sees *what* is loading (card / tab gets a pressed state + spinner);
 *   - marks the swap target (`aria-busy="true"` + `.neo-swap-busy`) so the pane
 *     being replaced dims (CSS delays the dim to avoid flicker on fast loads);
 *   - after `showDelay` (150ms) shows a thin trickling progress bar at the top;
 *   - after `slowDelay` (1.5s) adds `.is-slow` to #nav-busy ("Loading…" label);
 *   - reports page navigations to `onBusyChange`, for the navbar spinner.
 * Every start is paired with an end on htmx:afterRequest / sendError / sendAbort /
 * timeout (plus a hard safety timeout), so the indicator always finishes.
 *
 * Tracked requests: anything swapping into #config-content (page navigation), or
 * any element with `data-neo-progress` on itself / an ancestor. Opt out with
 * `data-neo-progress="false"`. Polls and background loads are not tracked.
 */
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  else root.NeoNavProgress = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
  'use strict';

  var SOURCE_ATTR = 'data-neo-loading';
  var TARGET_CLASS = 'neo-swap-busy';

  /**
   * Progress controller. Pure DOM/timer logic so it can be unit tested.
   * opts: { doc, setTimeout, clearTimeout, showDelay, slowDelay, trickleMs, maxMs, onBusyChange }
   */
  function createController(opts) {
    opts = opts || {};
    var doc = opts.doc;
    var setT = opts.setTimeout || setTimeout;
    var clearT = opts.clearTimeout || clearTimeout;
    var showDelay = opts.showDelay != null ? opts.showDelay : 150;
    var slowDelay = opts.slowDelay != null ? opts.slowDelay : 1500;
    var trickleMs = opts.trickleMs != null ? opts.trickleMs : 250;
    var maxMs = opts.maxMs != null ? opts.maxMs : 60000;

    var active = []; // { key, source, target, safety }
    var reportedBusy = false;
    var progress = 0;
    var visible = false;
    var showTimer = null;
    var slowTimer = null;
    var trickleTimer = null;
    var hideTimer = null;
    var barEl = null;

    function ensureBar() {
      if (barEl && barEl.parentNode) return barEl;
      barEl = doc.getElementById('neo-progress');
      if (!barEl) {
        barEl = doc.createElement('div');
        barEl.id = 'neo-progress';
        barEl.setAttribute('aria-hidden', 'true');
        var fill = doc.createElement('div');
        fill.className = 'neo-progress-fill';
        barEl.appendChild(fill);
        doc.body.appendChild(barEl);
      }
      return barEl;
    }

    function render() {
      var bar = ensureBar();
      var fill = bar.firstChild;
      if (fill && fill.style) fill.style.transform = 'scaleX(' + progress.toFixed(3) + ')';
    }

    function navBusy() {
      return doc.getElementById('nav-busy');
    }

    function refCount(el, attr, delta) {
      var n = parseInt(el.getAttribute(attr) || '0', 10) + delta;
      if (n > 0) el.setAttribute(attr, String(n));
      else el.removeAttribute(attr);
      return n;
    }

    function markSource(el, on) {
      if (!el || !el.setAttribute) return;
      var n = refCount(el, SOURCE_ATTR, on ? 1 : -1);
      if (n > 0) el.setAttribute('aria-busy', 'true');
      else el.removeAttribute('aria-busy');
    }

    function markTarget(el, on) {
      if (!el || !el.setAttribute) return;
      var n = refCount(el, 'data-neo-busy-count', on ? 1 : -1);
      if (n > 0) {
        el.setAttribute('aria-busy', 'true');
        el.classList.add(TARGET_CLASS);
      } else {
        el.removeAttribute('aria-busy');
        el.classList.remove(TARGET_CLASS);
      }
    }

    function trickle() {
      // Asymptotically approach 90%: fast at first, slower as it goes.
      var step = progress < 0.25 ? 0.08 : progress < 0.6 ? 0.04 : progress < 0.85 ? 0.015 : 0.004;
      progress = Math.min(0.94, progress + step);
      render();
      trickleTimer = setT(trickle, trickleMs);
    }

    function show() {
      showTimer = null;
      if (!active.length) return;
      if (hideTimer) {
        clearT(hideTimer);
        hideTimer = null;
      }
      var bar = ensureBar();
      visible = true;
      if (progress <= 0) progress = 0.12;
      bar.classList.remove('is-done');
      bar.classList.add('is-active');
      render();
      if (!trickleTimer) trickleTimer = setT(trickle, trickleMs);
    }

    function markSlow() {
      slowTimer = null;
      if (!active.length) return;
      var nb = navBusy();
      if (nb) nb.classList.add('is-slow');
      if (barEl) barEl.classList.add('is-slow');
    }

    function finish() {
      if (showTimer) clearT(showTimer);
      if (slowTimer) clearT(slowTimer);
      if (trickleTimer) clearT(trickleTimer);
      showTimer = slowTimer = trickleTimer = null;
      var nb = navBusy();
      if (nb) nb.classList.remove('is-slow');
      if (!visible) {
        progress = 0;
        return;
      }
      var bar = ensureBar();
      bar.classList.remove('is-slow');
      progress = 1;
      render();
      bar.classList.add('is-done');
      hideTimer = setT(function () {
        hideTimer = null;
        if (active.length) return;
        bar.classList.remove('is-active', 'is-done');
        visible = false;
        progress = 0;
        render();
      }, 300);
    }

    function indexOf(key) {
      for (var i = 0; i < active.length; i++) if (active[i].key === key) return i;
      return -1;
    }

    /** Tell the page when requests into #config-content start and stop. */
    function reportBusy() {
      var busy = active.some(function (e) {
        return e.target && e.target.id === 'config-content';
      });
      if (busy === reportedBusy) return;
      reportedBusy = busy;
      if (opts.onBusyChange) opts.onBusyChange(busy);
    }

    function start(key, info) {
      info = info || {};
      if (indexOf(key) !== -1) return;
      var entry = { key: key, source: info.source || null, target: info.target || null };
      markSource(entry.source, true);
      markTarget(entry.target, true);
      entry.safety = setT(function () {
        end(key);
      }, maxMs);
      active.push(entry);
      reportBusy();
      if (active.length === 1) {
        if (visible) {
          show();
        } else if (!showTimer) {
          showTimer = setT(show, showDelay);
        }
        if (!slowTimer) slowTimer = setT(markSlow, slowDelay);
      }
    }

    function end(key) {
      var i = indexOf(key);
      if (i === -1) return;
      var entry = active.splice(i, 1)[0];
      clearT(entry.safety);
      markSource(entry.source, false);
      markTarget(entry.target, false);
      reportBusy();
      if (!active.length) finish();
    }

    function endAll() {
      while (active.length) end(active[0].key);
    }

    return {
      start: start,
      end: end,
      endAll: endAll,
      activeCount: function () {
        return active.length;
      },
      isVisible: function () {
        return visible;
      },
      progress: function () {
        return progress;
      },
    };
  }

  /**
   * Clear any loading markers left in `doc`, independent of the controller's own
   * `active` bookkeeping.
   *
   * htmx bakes a literal snapshot of the *current* DOM into its history cache
   * right before every navigation that updates history (`htmx:beforeHistorySave`,
   * fired just before the in-flight request's response swaps in — i.e. while our
   * `data-neo-loading`/`neo-swap-busy` markers from that very navigation are still
   * on the page). A later Back/Forward restores that snapshot byte for byte, so
   * without this the pressed/spinner state on a clicked card or tab can come back
   * frozen — including on a phone, where "open a service, then hit Back" is the
   * normal gesture. Scrubbing here (not just calling ctl.endAll(), which only
   * knows about requests it is still tracking) covers markers on nodes the
   * controller never tracked, e.g. from before this scrub existed, or a snapshot
   * cached in a previous page load.
   */
  function scrubLoadingMarkers(doc) {
    try {
      var stale = doc.querySelectorAll('[' + SOURCE_ATTR + '], [data-neo-busy-count]');
      for (var i = 0; i < stale.length; i++) {
        var el = stale[i];
        el.removeAttribute(SOURCE_ATTR);
        el.removeAttribute('data-neo-busy-count');
        el.removeAttribute('aria-busy');
        el.classList.remove(TARGET_CLASS);
      }
      var bar = doc.getElementById('neo-progress');
      if (bar) {
        bar.classList.remove('is-active', 'is-done', 'is-slow');
        var fill = bar.firstChild;
        if (fill && fill.style) fill.style.transform = 'scaleX(0)';
      }
      var nb = doc.getElementById('nav-busy');
      if (nb) nb.classList.remove('is-slow');
    } catch (e) {}
  }

  function resolveEl(doc, t) {
    if (!t) return null;
    if (typeof t === 'string') {
      try {
        return doc.querySelector(t);
      } catch (e) {
        return null;
      }
    }
    return t.nodeType === 1 ? t : null;
  }

  function progressOptIn(el) {
    if (!el || !el.closest) return null;
    var host = el.closest('[data-neo-progress]');
    if (!host) return null;
    return host.getAttribute('data-neo-progress') !== 'false';
  }

  /** Is this request something the user is waiting on (navigation / opted-in)? */
  function isTracked(doc, detail) {
    var elt = resolveEl(doc, detail.elt);
    var optIn = progressOptIn(elt);
    if (optIn === false) return false;
    if (optIn === true) return true;
    var target = resolveEl(doc, detail.target) ||
      resolveEl(doc, detail.requestConfig && detail.requestConfig.target);
    return !!(target && target.id === 'config-content');
  }

  /** Visible element the user interacted with: the htmx elt, or the last click. */
  function pickSource(doc, elt, target, lastClick, now) {
    var hidden = function (el) {
      return !el || el === target || el.getAttribute('aria-hidden') === 'true' ||
        (el.classList && el.classList.contains('hidden'));
    };
    if (elt && !hidden(elt) && elt !== doc.body) return elt;
    if (lastClick && now - lastClick.at < 1000 && lastClick.el.isConnected !== false) {
      return lastClick.el;
    }
    return null;
  }

  /** Wire the controller to htmx events on `doc`. Returns the controller. */
  function install(doc, opts) {
    doc = doc || document;
    var ctl = createController(Object.assign({ doc: doc }, opts || {}));
    var lastClick = null;
    doc.addEventListener(
      'click',
      function (e) {
        var t = e.target && e.target.closest &&
          e.target.closest('button, a, [role="tab"], [hx-get], [hx-post], [data-hx-get]');
        if (t) lastClick = { el: t, at: Date.now() };
      },
      true
    );
    // Refresh after the click finished dispatching: a confirm() inside the
    // handler (unsaved changes) may have blocked for a while.
    if (typeof window !== 'undefined') {
      window.addEventListener('click', function () {
        if (lastClick) lastClick.at = Date.now();
      });
    }

    // Listen on document (bubble) so page handlers on body that cancel the
    // request (unsaved-changes confirm) have already run: check defaultPrevented.
    doc.addEventListener('htmx:beforeRequest', function (evt) {
      try {
        if (evt.defaultPrevented) return;
        var d = evt.detail || {};
        if (!d.xhr || !isTracked(doc, d)) return;
        var target = resolveEl(doc, d.target) ||
          resolveEl(doc, d.requestConfig && d.requestConfig.target);
        var elt = resolveEl(doc, d.elt);
        ctl.start(d.xhr, {
          source: pickSource(doc, elt, target, lastClick, Date.now()),
          target: target,
        });
        lastClick = null;
      } catch (e) {}
    });

    function stop(evt) {
      try {
        var d = evt.detail || {};
        if (d.xhr) ctl.end(d.xhr);
      } catch (e) {}
    }
    ['htmx:afterRequest', 'htmx:sendError', 'htmx:sendAbort', 'htmx:timeout', 'htmx:responseError']
      .forEach(function (name) {
        doc.addEventListener(name, stop);
      });

    function resetAll() {
      ctl.endAll();
      scrubLoadingMarkers(doc);
    }
    // Scrub *before* htmx serializes #config-content into its history cache
    // (see scrubLoadingMarkers) so a future Back/Forward restore is never
    // stuck mid-spinner. historyRestore/popstate/pageshow are belt-and-braces
    // for content already cached before this existed, or restored from a
    // server round-trip (cache miss) rather than the local snapshot.
    doc.addEventListener('htmx:beforeHistorySave', resetAll);
    doc.addEventListener('htmx:historyRestore', resetAll);
    window.addEventListener('popstate', resetAll);
    // Full page teardown (bfcache etc.): never leave a stuck bar.
    window.addEventListener('pageshow', function (e) {
      if (e.persisted) resetAll();
    });
    return ctl;
  }

  return {
    createController: createController,
    install: install,
    isTracked: isTracked,
    scrubLoadingMarkers: scrubLoadingMarkers,
  };
});

if (typeof document !== 'undefined' && typeof module === 'undefined') {
  window.neoNavProgress = window.NeoNavProgress.install(document, {
    onBusyChange: window.neoSetNavBusy,
  });
}
