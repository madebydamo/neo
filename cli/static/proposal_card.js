// Secret inputs on assistant proposal cards (templates/proposal_card.html.hbs).
// - Show / Generate buttons next to each input ([data-neo-ask-toggle|generate]);
//   Generate makes a random 24-character password in the browser.
// - Save / Save & apply ([data-neo-ask-save]) send the inputs with htmx
//   (hx-include); while one is empty the request is cancelled and the input
//   flagged. The server checks again.
(function (root, factory) {
  var api = factory();
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root && root.document) {
    root.NeoProposalCard = api;
    api.autostart(root.document);
  }
})(typeof window !== 'undefined' ? window : null, function () {
  var ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';

  /** Random alphanumeric string; `fill(Uint32Array)` is crypto.getRandomValues. */
  function generate(length, fill) {
    var n = length || 24;
    var out = '';
    // Rejection sampling keeps every character equally likely.
    var limit = Math.floor(0x100000000 / ALPHABET.length) * ALPHABET.length;
    var buf = new Uint32Array(n * 2);
    while (out.length < n) {
      fill(buf);
      for (var i = 0; i < buf.length && out.length < n; i++) {
        if (buf[i] < limit) out += ALPHABET[buf[i] % ALPHABET.length];
      }
    }
    return out;
  }

  /** Inputs (`{ value }`) still empty. */
  function missing(inputs) {
    return Array.prototype.filter.call(inputs, function (el) {
      return !String(el.value || '').trim();
    });
  }

  function cardOf(el) {
    return el && el.closest ? el.closest('.neo-proposal') : null;
  }

  function inputIn(card, name) {
    var all = card.querySelectorAll('[data-neo-ask]');
    for (var i = 0; i < all.length; i++) if (all[i].name === name) return all[i];
    return null;
  }

  function autostart(doc) {
    doc.addEventListener('click', function (e) {
      var btn = e.target.closest && e.target.closest('[data-neo-ask-toggle],[data-neo-ask-generate]');
      var card = cardOf(btn);
      if (!card) return;
      var gen = btn.hasAttribute('data-neo-ask-generate');
      var input = inputIn(card, btn.getAttribute(gen ? 'data-neo-ask-generate' : 'data-neo-ask-toggle'));
      if (!input) return;
      var toggle = card.querySelector('[data-neo-ask-toggle="' + input.name + '"]');
      if (gen) {
        input.value = generate(24, function (b) { return globalThis.crypto.getRandomValues(b); });
        input.classList.remove('input-error');
      }
      // A generated value is shown so the user can note it down.
      var show = gen || input.type === 'password';
      input.type = show ? 'text' : 'password';
      if (toggle) toggle.textContent = show ? 'Hide' : 'Show';
    });
    doc.addEventListener('input', function (e) {
      if (e.target.hasAttribute && e.target.hasAttribute('data-neo-ask') && String(e.target.value).trim()) {
        e.target.classList.remove('input-error');
      }
    });
    doc.addEventListener('htmx:beforeRequest', function (e) {
      var elt = e.detail && e.detail.elt;
      if (!elt || !elt.hasAttribute || !elt.hasAttribute('data-neo-ask-save')) return;
      var card = cardOf(elt);
      if (!card) return;
      var empty = missing(card.querySelectorAll('[data-neo-ask]'));
      if (!empty.length) return;
      e.preventDefault();
      empty.forEach(function (el) { el.classList.add('input-error'); });
      empty[0].focus();
      var hint = card.querySelector('[data-neo-ask-hint]');
      if (hint) hint.classList.add('text-error');
    });
  }

  return { generate: generate, missing: missing, autostart: autostart };
});
