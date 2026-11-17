// security_report.js — the security report page (neo security serve,
// templates/security/report.html.hbs): live check state, Check now, the
// decisions on a finding, and opening the finding a #f-<id> link points at.
//
// Buttons: [data-security-check] starts a check, [data-security-reload]
// reloads. Alpine handlers call NeoSecurityReport.decide(id, action, reason).
(function (root, factory) {
  var api = factory((root && root.NeoUtil) || require('./neo_util.js'));
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root && root.document) {
    root.NeoSecurityReport = api;
    api.autostart(root);
  }
})(typeof window !== 'undefined' ? window : null, function (NeoUtil) {
  'use strict';

  var POLL_MS = 10000;

  /**
   * What the header shows after a poll of /status: 'busy' while a check runs or
   * is requested, then 'done' when it produced a new report (`shownFinished`
   * is the finished_at the page was rendered with), else 'idle'.
   */
  function stateAfter(st, shownFinished) {
    if (st.running || st.requested) return 'busy';
    return String(st.finished_at) !== String(shownFinished) ? 'done' : 'idle';
  }

  var win = null;

  function stateEl() {
    return win.document.getElementById('scan-state');
  }

  function paintState(mode) {
    var el = stateEl();
    if (!el) return;
    el.querySelectorAll('[data-when]').forEach(function (part) {
      var on = part.dataset.when === mode;
      part.classList.toggle('hidden', !on);
      part.classList.toggle('inline-flex', on && part.tagName === 'SPAN');
    });
  }

  // A finished check offers a reload instead of reloading itself, so an open
  // chat is not cut off.
  var polling = null;
  function poll() {
    if (polling) return;
    paintState('busy');
    polling = win.setInterval(function () {
      win
        .fetch('/status', { credentials: 'same-origin' })
        .then(function (r) {
          return NeoUtil.readJson(r);
        })
        .then(function (st) {
          var mode = stateAfter(st, stateEl().dataset.finished);
          if (mode === 'busy') return;
          win.clearInterval(polling);
          polling = null;
          paintState(mode);
        })
        .catch(function () { /* keep polling */ });
    }, POLL_MS);
  }

  function post(path, body) {
    return win.fetch(path, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  }

  /** Accept / undo / review again, then reload on this finding. */
  function decide(id, action, reason) {
    return post('/findings/' + id + '/' + action, action === 'accept' ? { reason: reason || '' } : {}).then(
      function (r) {
        if (r.ok) {
          win.location.hash = 'f-' + id;
          win.location.reload();
          return;
        }
        return r.text().then(function (t) {
          throw new Error(t || r.statusText);
        });
      }
    );
  }

  function requestCheck(btn) {
    btn.disabled = true;
    post('/scan', {})
      .then(function (r) {
        if (r.ok) return poll();
        return r.text().then(function (t) {
          throw new Error(t);
        });
      })
      .catch(function (e) {
        btn.disabled = false;
        btn.title = 'Could not start a check: ' + e.message;
        btn.classList.add('text-error');
      });
  }

  /** `@` chips and links point at #f-<id>: open that finding. */
  function openFinding() {
    var doc = win.document;
    var hash = win.location.hash;
    var d = hash.indexOf('#f-') === 0 && doc.getElementById(hash.slice(1));
    if (!d || d.tagName !== 'DETAILS') return;
    // Filtered out (e.g. a harmless finding under Open): show everything.
    if (d.offsetParent === null) {
      var all = doc.getElementById('show-all');
      if (all) all.click();
    }
    d.open = true;
    win.requestAnimationFrame(function () {
      d.scrollIntoView({ block: 'start' });
    });
  }

  function localTimes(doc) {
    doc.querySelectorAll('time[data-ts]').forEach(function (t) {
      var s = Number(t.dataset.ts);
      if (!s) return;
      var d = new Date(s * 1000);
      t.dateTime = d.toISOString();
      t.textContent = d.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
      t.title = d.toString();
    });
  }

  function autostart(w) {
    win = w;
    var doc = w.document;
    w.addEventListener('hashchange', openFinding);
    doc.addEventListener('click', function (e) {
      var t = e.target;
      if (!t || !t.closest) return;
      var check = t.closest('[data-security-check]');
      if (check) return requestCheck(check);
      if (t.closest('[data-security-reload]')) w.location.reload();
    });
    NeoUtil.onReady(doc, function () {
      var el = stateEl();
      if (el && el.dataset.scanning) poll();
      else paintState('idle');
      openFinding();
      localTimes(doc);
    });
  }

  return {
    stateAfter: stateAfter,
    decide: decide,
    autostart: autostart,
  };
});
