// Small helpers shared by every page's scripts. Loaded first on both pages
// (configuration, navigator), so it must not touch document.body at load time.
(function (root, factory) {
  var api = factory();
  if (root) {
    root.NeoUtil = api;
    root.neoToast = api.toast;
  }
  if (typeof module === 'object' && module.exports) module.exports = api;
})(typeof window !== 'undefined' ? window : null, function () {
  /** Escape text for HTML body and attribute contexts. */
  function esc(s) {
    return String(s == null ? '' : s)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#39;');
  }

  /**
   * Show a transient alert in #toast-host (outside swapped content). Does nothing
   * on a page without a host.
   * @param {string} message
   * @param {'success'|'error'|'info'|'warning'} [type]
   */
  function toast(message, type) {
    var host = document.getElementById('toast-host');
    if (!host) return;
    var kind = type || 'info';
    var alertClass =
      kind === 'success'
        ? 'alert-success'
        : kind === 'error'
          ? 'alert-error'
          : kind === 'warning'
            ? 'alert-warning'
            : 'alert-info';
    var el = document.createElement('div');
    el.className = 'alert ' + alertClass + ' shadow-lg text-sm max-w-sm';
    el.setAttribute('role', 'status');

    var msg = document.createElement('span');
    msg.textContent = String(message == null ? '' : message);
    el.appendChild(msg);

    var dismiss = document.createElement('button');
    dismiss.type = 'button';
    dismiss.className = 'btn btn-ghost btn-xs btn-circle';
    dismiss.setAttribute('aria-label', 'Dismiss');
    dismiss.textContent = '✕';
    dismiss.addEventListener('click', function () {
      if (el.parentNode) el.parentNode.removeChild(el);
    });
    el.appendChild(dismiss);

    host.appendChild(el);
    setTimeout(function () {
      if (el.parentNode) el.parentNode.removeChild(el);
    }, 4000);
  }

  /**
   * Browser storage that never throws (private windows, blocked site data):
   * reads fall back, writes are dropped.
   * @param {'local'|'session'} kind
   * @param {Window} [win]
   */
  function store(kind, win) {
    function area() {
      return (win || window)[kind + 'Storage'];
    }
    function get(key) {
      try {
        return area().getItem(key);
      } catch (e) {
        return null;
      }
    }
    function set(key, value) {
      try {
        area().setItem(key, value);
      } catch (e) {}
    }
    return {
      get: get,
      set: set,
      remove: function (key) {
        try {
          area().removeItem(key);
        } catch (e) {}
      },
      getJson: function (key, fallback) {
        try {
          var raw = get(key);
          return raw ? JSON.parse(raw) : fallback;
        } catch (e) {
          return fallback;
        }
      },
      setJson: function (key, value) {
        set(key, JSON.stringify(value));
      },
    };
  }

  /** Run fn once the document is parsed (right away when it already is). */
  function onReady(doc, fn) {
    if (doc.readyState === 'loading') doc.addEventListener('DOMContentLoaded', fn);
    else fn();
  }

  /**
   * Mark a home-screen launch (iOS sets navigator.standalone; others use
   * display-mode). Call it from a script at the end of the body.
   */
  function markStandalone() {
    try {
      var standalone =
        window.navigator.standalone === true ||
        window.matchMedia('(display-mode: standalone)').matches ||
        window.matchMedia('(display-mode: fullscreen)').matches ||
        window.matchMedia('(display-mode: minimal-ui)').matches;
      if (!standalone) return;
      document.documentElement.classList.add('neo-standalone');
      if (document.body) document.body.classList.add('neo-standalone');
    } catch (e) {}
  }

  /**
   * Reload the open option pane from the server. The leave guard in
   * configuration.js asks once when the form holds unsaved edits, so callers
   * do not confirm themselves.
   * @returns {boolean} false when there is no pane to reload
   */
  function reloadPane() {
    var pane = document.getElementById('options-pane');
    var url = pane && pane.dataset && pane.dataset.loadUrl;
    if (!url || typeof htmx === 'undefined') return false;
    htmx.ajax('GET', url, { target: '#config-content', swap: 'innerHTML' });
    return true;
  }

  /**
   * Body of a fetch Response as JSON. Rejects with the server's `error` field, or
   * `<fallback> (<status>).` for any other failure (a proxy's HTML error page, …).
   */
  function readJson(res, fallback) {
    var generic = (fallback || 'Request failed') + ' (' + res.status + ').';
    return res.json().then(
      function (j) {
        if (!res.ok) throw new Error((j && j.error) || generic);
        return j;
      },
      function () {
        throw new Error(generic);
      }
    );
  }

  /** Did an htmx swap into this target replace the page content or the option pane? */
  function isContentSwap(target) {
    return (
      target.id === 'config-content' ||
      !!(target.closest && target.closest('#config-content') && target.id === 'options-pane')
    );
  }

  /**
   * Build an element. props: class, text, attrs {name: value}, on {event: fn}; any
   * other key is set as a property (id, type, title, disabled, …). Empty children
   * are skipped.
   */
  function h(tag, props, ...children) {
    var el = document.createElement(tag);
    Object.keys(props || {}).forEach(function (k) {
      var v = props[k];
      if (k === 'class') el.className = v;
      else if (k === 'text') el.textContent = v;
      else if (k === 'attrs') Object.keys(v).forEach(function (a) { el.setAttribute(a, v[a]); });
      else if (k === 'on') Object.keys(v).forEach(function (e) { el.addEventListener(e, v[e]); });
      else el[k] = v;
    });
    children.forEach(function (c) {
      if (c) el.appendChild(c);
    });
    return el;
  }

  return {
    esc: esc,
    toast: toast,
    store: store,
    onReady: onReady,
    markStandalone: markStandalone,
    reloadPane: reloadPane,
    readJson: readJson,
    isContentSwap: isContentSwap,
    h: h,
  };
});
