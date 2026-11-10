// Pages rendered from an older evaluation (docs/design/eval-latency.md §7).
//
// After a settings save the server keeps answering from the last evaluation,
// with the saved values laid over it, and evaluates in the background. When the
// new evaluation is served, #neo-eval-sync is replaced over /ws/status:
//   data-gen              the generation now served
//   data-changed          { key: generation whose result last changed }
//   data-changed-overlaid same, compared with what an overlaid page showed
//   data-pending          another evaluation is running or due
//   data-theme            the configured theme
// Every eval-backed fragment carries data-neo-eval-keys (space separated),
// data-neo-eval-gen, data-neo-eval-provisional and data-neo-eval-url. A fragment
// whose keys changed after its generation is fetched again in place; one with
// unsaved form edits only gets a note. The rest just take the new generation.
(function (root, factory) {
  var api = factory((root && root.NeoUtil) || require('./neo_util.js'));
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) {
    root.NeoEvalSync = api;
    api.autostart(root);
  }
})(typeof window !== 'undefined' ? window : null, function (NeoUtil) {
  'use strict';

  var NOTE_CLASS = 'neo-eval-stale-note';

  function parseMap(text) {
    try {
      var v = JSON.parse(text || '{}');
      return v && typeof v === 'object' ? v : {};
    } catch (e) {
      return {};
    }
  }

  /** #neo-eval-sync → { gen, pending, theme, changed, changedOverlaid }. */
  function readSync(el) {
    if (!el) return null;
    return {
      gen: Number(el.getAttribute('data-gen')) || 0,
      pending: el.getAttribute('data-pending') === 'true',
      theme: el.getAttribute('data-theme') || '',
      changed: parseMap(el.getAttribute('data-changed')),
      changedOverlaid: parseMap(el.getAttribute('data-changed-overlaid')),
    };
  }

  /** A fragment's keys / generation / provisional flag. */
  function readNode(el) {
    return {
      keys: String(el.getAttribute('data-neo-eval-keys') || '')
        .split(/\s+/)
        .filter(Boolean),
      gen: Number(el.getAttribute('data-neo-eval-gen')) || 0,
      provisional: el.getAttribute('data-neo-eval-provisional') === 'true',
      url: el.getAttribute('data-neo-eval-url') || '',
    };
  }

  /**
   * True when a fragment rendered from `node.gen` is out of date at `sync.gen`.
   * A provisional fragment already showed the saved values, so it is compared
   * with what the overlay predicted; a plain one with the raw results.
   */
  function needsRefresh(node, sync) {
    if (!sync || !node.gen || sync.gen <= node.gen) return false;
    var map = node.provisional ? sync.changedOverlaid : sync.changed;
    return node.keys.some(function (k) {
      return (Number(map[k]) || 0) > node.gen;
    });
  }

  /** Is the user typing in `el`? Refreshing would take the field away. */
  function typingIn(el, doc) {
    var a = doc.activeElement;
    return !!(a && a !== doc.body && el.contains(a) && /^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName));
  }

  function staleNote(el) {
    if (el.querySelector('.' + NOTE_CLASS)) return;
    var note = NeoUtil.h(
      'div',
      { class: NOTE_CLASS + ' alert alert-info alert-soft text-sm m-3', attrs: { role: 'status' } },
      NeoUtil.h('span', {
        text: 'This page changed with your latest settings. Save or discard your edits, then reload it.',
      }),
      NeoUtil.h('button', {
        type: 'button',
        class: 'btn btn-xs',
        text: 'Reload',
        on: {
          click: function () {
            NeoUtil.reloadPane();
          },
        },
      })
    );
    el.insertBefore(note, el.firstChild);
  }

  /**
   * Bring every eval-backed fragment of `doc` up to `sync`.
   * deps: { refresh(el, url), dirty() } (htmx and the form guard in the page).
   */
  function apply(doc, sync, deps) {
    if (!sync) return [];
    var refreshed = [];
    if (sync.theme && doc.body && doc.body.getAttribute('data-theme') !== sync.theme) {
      doc.body.setAttribute('data-theme', sync.theme);
    }
    var nodes = doc.querySelectorAll('[data-neo-eval-keys]');
    for (var i = 0; i < nodes.length; i++) {
      var el = nodes[i];
      var node = readNode(el);
      if (!node.gen || sync.gen <= node.gen) continue;
      if (!needsRefresh(node, sync)) {
        el.setAttribute('data-neo-eval-gen', String(sync.gen));
        if (!sync.pending) el.setAttribute('data-neo-eval-provisional', 'false');
        continue;
      }
      if (el.id === 'options-pane' && deps.dirty()) {
        staleNote(el);
        continue;
      }
      if (typingIn(el, doc)) {
        // Refresh once the field is left.
        el.addEventListener(
          'focusout',
          function () {
            apply(doc, readSync(doc.getElementById('neo-eval-sync')), deps);
          },
          { once: true }
        );
        continue;
      }
      if (!node.url) continue;
      // Mark it so a second push before the response does not fetch again.
      el.setAttribute('data-neo-eval-gen', String(sync.gen));
      deps.refresh(el, node.url);
      refreshed.push(node.url);
    }
    return refreshed;
  }

  function htmxDeps(win) {
    return {
      refresh: function (el, url) {
        if (typeof win.htmx === 'undefined') return;
        el.setAttribute('data-neo-progress', 'false');
        win.htmx.ajax('GET', url, { target: el, swap: 'outerHTML' });
      },
      dirty: function () {
        return typeof win.neoIsConfigFormDirty === 'function' && win.neoIsConfigFormDirty();
      },
    };
  }

  return {
    readSync: readSync,
    readNode: readNode,
    needsRefresh: needsRefresh,
    apply: apply,
    autostart: function (win) {
      var doc = win.document;
      if (!doc) return;
      var deps = htmxDeps(win);
      function run() {
        apply(doc, readSync(doc.getElementById('neo-eval-sync')), deps);
      }
      doc.addEventListener('htmx:oobAfterSwap', function (evt) {
        var t = evt && evt.detail && evt.detail.target;
        if (t && t.id === 'neo-eval-sync') run();
      });
      // A fragment loaded while a push was in flight may already be behind.
      doc.addEventListener('htmx:afterSettle', function (evt) {
        var t = evt && evt.detail && evt.detail.target;
        if (t && t.querySelector && t.querySelector('[data-neo-eval-keys]')) run();
      });
    },
  };
});
