// Operation locks in the UI (server side: cli/src/commands/web/locks.rs).
//
// - Any element with data-neo-lock="<scope>[:sh] …" needs those lock scopes
//   (exclusive unless ":sh"): `system`, `service/<name>`, `unit/<unit>`.
//   While a live holder conflicts, the element is disabled and its title says
//   why ("Blocked: Activation in progress (started 12:03)"); it is restored
//   when the holder is gone. No per-button code: annotate the element.
// - Holders come from #neo-locks[data-locks] (JSON), loaded once via GET /locks
//   and replaced over /ws/status whenever any process takes or drops a lock.
// - A 409 answer (a route refused because of a lock) becomes an error toast.
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) {
    root.NeoLocks = api;
    api.autostart(root);
  }
})(typeof window !== 'undefined' ? window : null, function () {
  'use strict';

  var UNIT_SUFFIX = /\.(service|target|timer|socket|mount|path|slice|scope)$/;

  /** `unit/docker-x` → `unit/docker-x.service` (same as the server's Scope::unit). */
  function normalizeScope(scope) {
    if (scope.indexOf('unit/') === 0 && !UNIT_SUFFIX.test(scope)) return scope + '.service';
    return scope;
  }

  /** "system:sh unit/x" → [{scope:'system',mode:'sh'},{scope:'unit/x.service',mode:'ex'}] */
  function parseNeeds(attr) {
    return String(attr || '')
      .split(/\s+/)
      .filter(Boolean)
      .map(function (tok) {
        var sh = /:sh$/.test(tok);
        return { scope: normalizeScope(sh ? tok.slice(0, -3) : tok), mode: sh ? 'sh' : 'ex' };
      });
  }

  function conflicts(a, b) {
    return a === 'ex' || b === 'ex';
  }

  /** First holder that blocks any of `needs`, or null. */
  function blocker(needs, holders) {
    for (var i = 0; i < needs.length; i++) {
      for (var j = 0; j < (holders || []).length; j++) {
        var h = holders[j];
        var scopes = h.scopes || [];
        for (var k = 0; k < scopes.length; k++) {
          if (scopes[k].scope === needs[i].scope && conflicts(scopes[k].mode, needs[i].mode)) return h;
        }
      }
    }
    return null;
  }

  function readHolders(doc) {
    var el = doc.getElementById('neo-locks');
    if (!el) return [];
    try {
      var v = JSON.parse(el.getAttribute('data-locks') || '[]');
      return Array.isArray(v) ? v : [];
    } catch (e) {
      return [];
    }
  }

  function lockEl(el, holder) {
    var msg = 'Blocked: ' + (holder.message || holder.label || 'another operation is running');
    if (!el.hasAttribute('data-neo-locked')) {
      el.setAttribute('data-neo-title', el.getAttribute('title') || '');
      el.setAttribute('data-neo-was-disabled', el.disabled ? '1' : '0');
      el.setAttribute('data-neo-had-btn-disabled', el.classList.contains('btn-disabled') ? '1' : '0');
    }
    el.setAttribute('data-neo-locked', '');
    el.disabled = true;
    el.classList.add('btn-disabled');
    el.setAttribute('title', msg);
  }

  function unlockEl(el) {
    if (!el.hasAttribute('data-neo-locked')) return;
    var title = el.getAttribute('data-neo-title');
    if (title) el.setAttribute('title', title);
    else el.removeAttribute('title');
    // Disabled by its own markup or an in-flight htmx request (hx-disabled-elt, which
    // re-enables it itself): leave `disabled` alone, but always drop the grey class
    // unless the markup had it.
    if (el.getAttribute('data-neo-was-disabled') !== '1') el.disabled = false;
    if (el.getAttribute('data-neo-had-btn-disabled') !== '1') el.classList.remove('btn-disabled');
    el.removeAttribute('data-neo-locked');
    el.removeAttribute('data-neo-title');
    el.removeAttribute('data-neo-was-disabled');
    el.removeAttribute('data-neo-had-btn-disabled');
  }

  /** Disable / restore every [data-neo-lock] element under `doc`. */
  function apply(doc) {
    var holders = readHolders(doc);
    var els = doc.querySelectorAll('[data-neo-lock]');
    for (var i = 0; i < els.length; i++) {
      var b = blocker(parseNeeds(els[i].getAttribute('data-neo-lock')), holders);
      if (b) lockEl(els[i], b);
      else unlockEl(els[i]);
    }
  }

  /** Message for a refused request, or null when `status` is not a lock conflict. */
  function conflictMessage(status, body) {
    if (status !== 409) return null;
    var text = String(body || '').trim();
    return text || 'Blocked: another operation is running';
  }

  function toast(win, msg) {
    if (typeof win.neoToast === 'function') win.neoToast(msg, 'error');
  }

  return {
    parseNeeds: parseNeeds,
    blocker: blocker,
    apply: apply,
    conflictMessage: conflictMessage,
    /**
     * For fetch() callers: toast and return true when `res` is a lock conflict.
     * @param {Response} res
     * @returns {Promise<boolean>}
     */
    handleFetch: function (res) {
      if (!res || res.status !== 409) return Promise.resolve(false);
      return res.text().then(function (t) {
        toast(root(), conflictMessage(409, t));
        return true;
      });
    },
    autostart: function (win) {
      var doc = win.document;
      if (!doc) return;
      var pending = null;
      function schedule() {
        if (pending) return;
        pending = win.setTimeout(function () {
          pending = null;
          apply(doc);
        }, 0);
      }
      doc.addEventListener('htmx:responseError', function (evt) {
        var xhr = evt && evt.detail && evt.detail.xhr;
        var msg = xhr && conflictMessage(xhr.status, xhr.responseText);
        if (msg) toast(win, msg);
      });
      ['htmx:afterSettle', 'htmx:oobAfterSwap', 'htmx:afterSwap'].forEach(function (ev) {
        doc.addEventListener(ev, schedule);
      });
      if (doc.readyState === 'loading') doc.addEventListener('DOMContentLoaded', schedule);
      else schedule();
      // Alpine-rendered buttons (versioning tab) appear without an htmx swap.
      if (typeof win.MutationObserver === 'function') {
        var mo = new win.MutationObserver(function (records) {
          for (var i = 0; i < records.length; i++) {
            var t = records[i].target;
            if (t && t.id === 'neo-locks') return schedule();
            var added = records[i].addedNodes || [];
            for (var j = 0; j < added.length; j++) {
              var n = added[j];
              if (n.nodeType === 1 && (n.hasAttribute('data-neo-lock') || (n.querySelector && n.querySelector('[data-neo-lock]')))) {
                return schedule();
              }
            }
          }
        });
        var start = function () {
          if (doc.body) mo.observe(doc.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['data-locks'] });
        };
        if (doc.body) start();
        else doc.addEventListener('DOMContentLoaded', start);
      }
    },
  };

  function root() {
    return typeof window !== 'undefined' ? window : {};
  }
});
