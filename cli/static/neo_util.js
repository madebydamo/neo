// Small helpers shared by every page's scripts. Loaded first on both pages
// (configuration, navigator), so it must not touch document.body at load time.
// Also wires the generic markup hooks every page may use (wireHooks).
(function (root, factory) {
  var api = factory();
  if (root) {
    root.NeoUtil = api;
    root.neoToast = api.toast;
    if (root.document) api.wireHooks(root.document);
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

  /**
   * In-page replacement for window.confirm / alert / prompt: a modal <dialog> made
   * of ordinary buttons, so browser-driving agents (which cannot reach the
   * browser's own dialogs) can answer it too. A message whose first paragraph is
   * followed by a blank line shows that paragraph as the title.
   * opts: message, okLabel ('OK'), cancelLabel ('Cancel'), cancel (false: no
   * Cancel button), danger (red OK button), input (string: show a text field
   * with this value).
   * Resolves true / false, or with input the text / null when cancelled.
   */
  function dialog(opts) {
    opts = opts || {};
    var doc = document;
    var paras = String(opts.message == null ? '' : opts.message).split(/\n\s*\n/);
    var title = paras.length > 1 ? paras.shift() : '';
    var asks = opts.input != null;
    return new Promise(function (resolve) {
      var settled = false;
      var dlg;
      function done(ok) {
        if (settled) return;
        settled = true;
        if (dlg.open) dlg.close();
        dlg.remove();
        if (asks) resolve(ok ? field.value : null);
        else resolve(ok);
      }
      var field = asks
        ? h('input', { type: 'text', class: 'input input-bordered input-sm w-full mt-3', value: String(opts.input), attrs: { 'aria-label': title || paras.join(' ') } })
        : null;
      var cancel = opts.cancel === false
        ? null
        : h('button', { type: 'button', class: 'btn btn-sm btn-ghost', text: opts.cancelLabel || 'Cancel', attrs: { 'data-neo-dialog': 'cancel' }, on: { click: function () { done(false); } } });
      var ok = h('button', {
        type: 'submit',
        class: 'btn btn-sm ' + (opts.danger ? 'btn-error' : 'btn-primary'),
        text: opts.okLabel || 'OK',
        attrs: { 'data-neo-dialog': 'ok' },
      });
      var box = h(
        'form',
        { class: 'modal-box max-w-md', attrs: { method: 'dialog' }, on: { submit: function (e) { e.preventDefault(); done(true); } } },
        title ? h('h3', { class: 'font-bold text-lg mb-2', text: title }) : null,
        h('p', { class: 'text-sm whitespace-pre-line', text: paras.join('\n\n') }),
        field,
        h('div', { class: 'modal-action' }, cancel, ok)
      );
      dlg = h(
        'dialog',
        { class: 'modal modal-bottom sm:modal-middle', attrs: { role: 'alertdialog', 'data-neo-dialog': '' } },
        box,
        h('div', { class: 'modal-backdrop', on: { click: function () { done(opts.cancel === false); } } })
      );
      // Escape: the browser fires cancel, then close.
      dlg.addEventListener('cancel', function (e) {
        e.preventDefault();
        done(opts.cancel === false);
      });
      doc.body.appendChild(dlg);
      if (dlg.showModal) dlg.showModal();
      else dlg.setAttribute('open', '');
      (field || ok).focus();
      if (field) field.select();
    });
  }

  /** In-page confirm: resolves true on OK. opts as for dialog. */
  function confirm(message, opts) {
    return dialog(Object.assign({}, opts, { message: message }));
  }

  /** In-page alert with a single OK button. */
  function alert(message, opts) {
    return dialog(Object.assign({}, opts, { message: message, cancel: false })).then(function () {});
  }

  /** In-page prompt: resolves the entered text, or null when cancelled. */
  function prompt(message, value, opts) {
    return dialog(Object.assign({}, opts, { message: message, input: value == null ? '' : value }));
  }

  /**
   * Checks every htmx request passes before it is sent (unsaved-changes guard).
   * fn(detail) gets htmx:confirm's detail (elt, target, verb, path) and returns
   * true/undefined to let it through, false to drop it, or a Promise<boolean>
   * (e.g. from NeoUtil.confirm). Guards run in order, before hx-confirm's question.
   */
  var requestGuards = [];
  function guardRequests(fn) {
    requestGuards.push(fn);
  }

  /** Run checks in order: true / false when all answer at once, else a Promise. */
  function runChecks(checks) {
    for (var i = 0; i < checks.length; i++) {
      var r = checks[i]();
      if (r === false) return false;
      if (r && typeof r.then === 'function') {
        var rest = checks.slice(i + 1);
        return r.then(function (ok) {
          return ok ? runChecks(rest) : false;
        });
      }
    }
    return true;
  }

  /** OK label for an hx-confirm dialog: the button's own short text or label. */
  function confirmLabel(elt) {
    if (!elt || !elt.getAttribute) return 'OK';
    var own = elt.getAttribute('data-neo-confirm-ok');
    if (own) return own;
    var text = (elt.textContent || '').replace(/\s+/g, ' ').trim();
    if (text && text.length <= 32) return text;
    return elt.getAttribute('aria-label') || elt.getAttribute('title') || 'OK';
  }

  /** Red OK button: destructive buttons, or a question about deleting or erasing. */
  function confirmDanger(elt, question) {
    if (elt && elt.closest && elt.closest('.btn-error, [class*="text-error"]')) return true;
    return /cannot be undone|delete|erase|discard/i.test(question || '');
  }

  /**
   * htmx:confirm: run the request guards, then ask hx-confirm's question in an
   * in-page dialog instead of window.confirm. issueRequest(true) sends the
   * request without htmx asking again.
   */
  function onHtmxConfirm(e) {
    var d = e.detail || {};
    var checks = requestGuards.map(function (g) {
      return function () {
        return g(d);
      };
    });
    if (d.question) {
      checks.push(function () {
        return confirm(d.question, { okLabel: confirmLabel(d.elt), danger: confirmDanger(d.elt, d.question) });
      });
    }
    var r = runChecks(checks);
    if (r === true) return;
    e.preventDefault();
    if (r === false) return;
    r.then(function (ok) {
      if (ok) d.issueRequest(true);
    });
  }

  /**
   * Delegated handlers for markup hooks, on the document so swapped-in content
   * works too:
   * - [data-neo-reload]: click reloads the page.
   * - [data-neo-scroll-to="<id>"]: click scrolls smoothly to that element
   *   (in-page links that must not change the URL hash).
   * - img[data-neo-icon-fallback]: a broken icon hides itself and shows its next
   *   sibling (the placeholder glyph).
   * - [data-neo-copy="<id>"]: click copies that element's text to the clipboard;
   *   the button says "Copied" for a moment.
   * - [data-neo-remove-closest="<selector>"]: click removes the closest ancestor
   *   matching the selector (e.g. Cancel of an inline form).
   * - hx-confirm: asked in an in-page dialog (onHtmxConfirm), never window.confirm;
   *   the OK button reads data-neo-confirm-ok, else the button's text.
   */
  function copyText(doc, button) {
    var src = doc.getElementById(button.getAttribute('data-neo-copy'));
    var clip = doc.defaultView.navigator && doc.defaultView.navigator.clipboard;
    if (!src || !clip) return;
    if (button.neoCopyLabel == null) button.neoCopyLabel = button.textContent;
    clip.writeText(src.textContent.trim()).then(function () {
      button.textContent = 'Copied';
      doc.defaultView.setTimeout(function () {
        button.textContent = button.neoCopyLabel;
      }, 1500);
    });
  }

  function wireHooks(doc) {
    doc.addEventListener('htmx:confirm', onHtmxConfirm);
    doc.addEventListener('click', function (e) {
      var t = e.target;
      if (!t || !t.closest) return;
      if (t.closest('[data-neo-reload]')) {
        doc.defaultView.location.reload();
        return;
      }
      var link = t.closest('[data-neo-scroll-to]');
      if (link) {
        e.preventDefault();
        var target = doc.getElementById(link.getAttribute('data-neo-scroll-to'));
        if (target) target.scrollIntoView({ behavior: 'smooth', block: 'start' });
        return;
      }
      var copy = t.closest('[data-neo-copy]');
      if (copy) {
        copyText(doc, copy);
        return;
      }
      var remover = t.closest('[data-neo-remove-closest]');
      if (remover) {
        var gone = remover.closest(remover.getAttribute('data-neo-remove-closest'));
        if (gone) gone.remove();
      }
    });
    // error does not bubble: listen in the capture phase.
    doc.addEventListener(
      'error',
      function (e) {
        var img = e.target;
        if (!img || img.tagName !== 'IMG' || !img.hasAttribute('data-neo-icon-fallback')) return;
        img.style.display = 'none';
        if (img.nextElementSibling) img.nextElementSibling.style.display = 'flex';
      },
      true
    );
  }

  return {
    wireHooks: wireHooks,
    esc: esc,
    toast: toast,
    store: store,
    onReady: onReady,
    markStandalone: markStandalone,
    reloadPane: reloadPane,
    readJson: readJson,
    isContentSwap: isContentSwap,
    h: h,
    dialog: dialog,
    confirm: confirm,
    alert: alert,
    prompt: prompt,
    guardRequests: guardRequests,
    runChecks: runChecks,
    confirmLabel: confirmLabel,
  };
});
