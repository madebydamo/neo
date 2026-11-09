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

  return { esc: esc, toast: toast };
});
