/* Changes preview (pending changes / version compare) — delegated handlers.
 *
 * Markup is rendered server-side (cli/src/commands/web/diff/render.rs) and swapped into
 * #changes-body by htmx or innerHTML, so everything here is delegated on `document`
 * and keyed on data-diff-* attributes:
 *   data-diff-jump="<file id>"   tree entry → open + scroll to that file's diff
 *   data-diff-expand             reveal the folded unchanged lines right after it
 *   data-diff-view="unified|split"  layout toggle (remembered per browser)
 *   data-diff-toggle-all="open|close"  expand / collapse every file
 *   data-diff-close-modal        close the dialog (service links navigate via htmx)
 */
(function () {
  'use strict';

  var VIEW_KEY = 'neo.diffView';
  var storage = window.NeoUtil.store('local');

  function readView() {
    return storage.get(VIEW_KEY) === 'split' ? 'split' : 'unified';
  }

  function syncViewButtons(root, view) {
    var btns = (root || document).querySelectorAll('[data-diff-view]');
    for (var i = 0; i < btns.length; i++) {
      var on = btns[i].getAttribute('data-diff-view') === view;
      btns[i].setAttribute('aria-pressed', on ? 'true' : 'false');
      btns[i].classList.toggle('btn-active', on);
    }
  }

  function applyView(view) {
    document.documentElement.setAttribute('data-neo-diff-view', view);
    syncViewButtons(document, view);
  }

  function setView(view) {
    storage.set(VIEW_KEY, view);
    applyView(view);
  }

  function jumpTo(id, trigger) {
    var target = document.getElementById(id);
    if (!target) return;
    if (target.tagName === 'DETAILS') target.open = true;
    var scope = target.closest('[data-neo-changes]') || document;
    var active = scope.querySelectorAll('.neo-tree-file.is-active');
    for (var i = 0; i < active.length; i++) active[i].classList.remove('is-active');
    if (trigger) trigger.classList.add('is-active');
    var reduce = window.matchMedia && window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    target.scrollIntoView({ block: 'start', behavior: reduce ? 'auto' : 'smooth' });
    target.classList.remove('is-flash');
    // Restart the highlight animation.
    void target.offsetWidth;
    target.classList.add('is-flash');
    var summary = target.querySelector('summary');
    if (summary) summary.focus({ preventScroll: true });
  }

  function expandFold(btn) {
    var row = btn.closest('tbody');
    if (!row) return;
    var body = row.nextElementSibling;
    if (body) body.hidden = false;
    row.parentNode.removeChild(row);
  }

  function toggleAll(btn, open) {
    var scope = btn.closest('[data-neo-changes]') || document;
    var files = scope.querySelectorAll('details[data-diff-file]');
    for (var i = 0; i < files.length; i++) files[i].open = open;
  }

  document.addEventListener('click', function (ev) {
    var t = ev.target;
    if (!(t instanceof Element)) return;
    var el;
    if ((el = t.closest('[data-diff-jump]'))) {
      ev.preventDefault();
      jumpTo(el.getAttribute('data-diff-jump'), el);
    } else if ((el = t.closest('[data-diff-expand]'))) {
      ev.preventDefault();
      expandFold(el);
    } else if ((el = t.closest('[data-diff-view]'))) {
      ev.preventDefault();
      setView(el.getAttribute('data-diff-view'));
    } else if ((el = t.closest('[data-diff-toggle-all]'))) {
      ev.preventDefault();
      toggleAll(el, el.getAttribute('data-diff-toggle-all') === 'open');
    } else if ((el = t.closest('[data-diff-close-modal]'))) {
      var dlg = el.closest('dialog');
      if (dlg && typeof dlg.close === 'function') dlg.close();
    }
  });

  // Server markup always says "unified"; fix the pressed state when a preview appears.
  function watch() {
    applyView(readView());
    var host = document.getElementById('changes-body');
    if (!host || typeof MutationObserver === 'undefined') return;
    new MutationObserver(function () {
      if (host.querySelector('[data-neo-changes]')) syncViewButtons(host, readView());
    }).observe(host, { childList: true });
  }

  window.NeoUtil.onReady(document, watch);
})();
