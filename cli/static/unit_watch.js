// unit_watch.js
// Live unit state on the option pane: which units the status socket reports, and the
// summary badge folded from them.

(function () {
  // Live unit status over /ws/status:
  // When an option pane with runtime units is shown, register those units with the
  // server so it polls systemctl ActiveState while this WebSocket stays open and
  // pushes OOB control HTML on every change. Leaving the pane (or disconnect)
  // clears interest so we do not poll forever.
  var wsWrapper = null;
  var watchedUnits = [];

  function neoSendUnitWatch(op, units) {
    if (!wsWrapper || !units) return;
    try {
      wsWrapper.send(JSON.stringify({ op: op, units: units }));
    } catch (e) {}
  }

  function neoCollectUnitsFrom(root) {
    if (!root || !root.querySelectorAll) return [];
    var rows = root.querySelectorAll('.unit-row[data-unit]');
    var out = [];
    for (var i = 0; i < rows.length; i++) {
      var n = rows[i].getAttribute('data-unit');
      if (n) out.push(n);
    }
    return out;
  }

  function neoSyncWatchedUnits(root) {
    var next = neoCollectUnitsFrom(root || document.getElementById('config-content'));
    // Replace interest set on the server (handles pane switches cleanly).
    neoSendUnitWatch('watch_replace', next);
    watchedUnits = next;
  }

  document.body.addEventListener('htmx:wsOpen', function (evt) {
    try {
      wsWrapper = evt.detail && evt.detail.socketWrapper;
      // Re-register after reconnect (htmx ws extension reconnects automatically).
      if (watchedUnits && watchedUnits.length) {
        neoSendUnitWatch('watch_replace', watchedUnits);
      } else {
        neoSyncWatchedUnits(document.getElementById('config-content'));
      }
    } catch (e) {}
  });

  document.body.addEventListener('htmx:wsClose', function () {
    wsWrapper = null;
  });

  document.body.addEventListener('htmx:afterSwap', function (evt) {
    try {
      var t = evt.detail && evt.detail.target;
      if (!t) return;
      // Option pane (or grid without units) landed in #config-content.
      if (NeoUtil.isContentSwap(t)) {
        neoSyncWatchedUnits(document.getElementById('config-content'));
      }
    } catch (e) {}
  });

  // If the option pane was already present when the script ran (unlikely), sync once.
  document.addEventListener('DOMContentLoaded', function () {
    neoSyncWatchedUnits(document.getElementById('config-content'));
  });
})();

// Unit status summary: folds the live per-unit ActiveState (pushed into
// #unit-controls-* over /ws/status) into the pane header badge and Status tab dot.
(function unitSummary() {
  var scheduled = false;

  function summarize() {
    scheduled = false;
    var root = document.getElementById('runtime-units');
    var badges = document.querySelectorAll('[data-unit-summary]');
    var dots = document.querySelectorAll('[data-unit-summary-dot]');
    if (!root || (!badges.length && !dots.length)) return;
    // Same summarizer as the services grid / sidebar dots (static/service_status.js).
    var ctrls = root.querySelectorAll('.unit-row .unit-controls');
    var entries = [];
    for (var i = 0; i < ctrls.length; i++) {
      entries.push({
        state: ctrls[i].getAttribute('data-active-state'),
        health: ctrls[i].getAttribute('data-health'),
        timer: ctrls[i].closest('.unit-row').hasAttribute('data-timer'),
      });
    }
    var summary = window.NeoUnitHealth
      ? window.NeoUnitHealth.summarize(entries)
      : { state: 'unknown', text: 'checking…' };
    for (var d = 0; d < dots.length; d++) {
      dots[d].setAttribute('data-state', summary.state);
      dots[d].setAttribute('title', summary.text);
    }
    for (var b = 0; b < badges.length; b++) {
      var t = badges[b].querySelector('[data-unit-summary-text]');
      if (t) t.textContent = summary.text;
    }
  }

  function schedule() {
    if (scheduled) return;
    scheduled = true;
    (window.requestAnimationFrame || setTimeout)(summarize);
  }

  function start() {
    var host = document.getElementById('config-content') || document.body;
    if (typeof MutationObserver === 'function') {
      new MutationObserver(schedule).observe(host, {
        subtree: true,
        childList: true,
        attributes: true,
        attributeFilter: ['data-active-state', 'data-health'],
      });
    }
    schedule();
  }

  window.NeoUtil.onReady(document, start);
})();
