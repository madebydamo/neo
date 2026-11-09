// Plugin pills: enable an ad-board ticker only when the label overflows one line.
(function pluginPills() {
  var observed = typeof WeakSet === 'function' ? new WeakSet() : null;

  function measure(pill) {
    var text = pill.querySelector('.plugin-pill-text');
    if (!text) return;
    var clip = pill.querySelector('.plugin-pill-clip') || pill;
    var box = clip.clientWidth;
    var overflow = text.offsetWidth > box + 0.5;
    if (overflow) {
      pill.classList.add('is-overflow');
      var seconds = Math.max(6, Math.min(18, text.offsetWidth / 24));
      pill.style.setProperty('--plugin-pill-duration', seconds.toFixed(1) + 's');
    } else {
      pill.classList.remove('is-overflow');
      pill.style.removeProperty('--plugin-pill-duration');
    }
  }

  var ro =
    typeof ResizeObserver === 'function'
      ? new ResizeObserver(function (entries) {
          for (var i = 0; i < entries.length; i++) {
            measure(entries[i].target);
          }
        })
      : null;

  function observe(root) {
    var scope = root && root.querySelectorAll ? root : document;
    var pills = scope.querySelectorAll('.plugin-pill');
    for (var i = 0; i < pills.length; i++) {
      var el = pills[i];
      if (ro && (!observed || !observed.has(el))) {
        if (observed) observed.add(el);
        ro.observe(el);
      }
      measure(el);
    }
  }

  function start() {
    observe(document);
    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(function () {
        observe(document);
      });
    }
  }

  window.NeoUtil.onReady(document, start);

  document.body.addEventListener('htmx:afterSwap', function (evt) {
    var t = evt.detail && evt.detail.target;
    observe(t && t.querySelectorAll ? t : document);
  });
})();
