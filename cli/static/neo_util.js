// Small helpers shared by every page's scripts. Loaded first on both pages
// (configuration, navigator), so it must not touch document.body at load time.
(function (root, factory) {
  var api = factory();
  if (root) root.NeoUtil = api;
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

  return { esc: esc };
});
