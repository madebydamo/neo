// configuration.js
// Client helpers for the homeserver config page: activation resume, live unit logs (SSE),
// and WebSocket unit-watch registration. Loaded only by configuration.html.hbs.

(function () {
  function resume() {
    try {
      const raw = localStorage.getItem('neo.pendingActivation');
      if (!raw) return;
      const data = JSON.parse(raw);
      if (data && data.id) {
        const modal = document.getElementById('changes-modal');
        if (modal) modal.showModal();
        htmx.ajax('GET', '/activation/monitor/' + encodeURIComponent(data.id), {
          target: '#changes-body',
          swap: 'innerHTML',
        });
      }
    } catch (e) {}
  }
  document.addEventListener('DOMContentLoaded', resume);
  document.addEventListener('htmx:afterSettle', resume);
  document.body.addEventListener('htmx:beforeSwap', function (e) {
    var l = document.getElementById('act-log');
    if (l && e.detail.target === l) {
      l.dataset.atBottom =
        l.scrollTop + l.clientHeight >= l.scrollHeight - 8 ? '1' : '';
    }
  });
  document.body.addEventListener('htmx:afterSwap', function (e) {
    var l = document.getElementById('act-log');
    if (l && e.detail.target === l && l.dataset.atBottom) {
      l.scrollTop = l.scrollHeight;
    }
  });

  window.openActivationSuccess = function (btn) {
    var mon = btn && btn.closest ? btn.closest('#activation-monitor') : null;
    var d = document.getElementById('activation-success');
    var b = document.getElementById('activation-success-body');
    if (!mon || !d || !b) return;
    var clone = mon.cloneNode(true);
    var actions = clone.querySelectorAll('[data-dialog-actions]');
    for (var i = 0; i < actions.length; i++) {
      actions[i].remove();
    }
    b.innerHTML = clone.innerHTML;
    try {
      localStorage.removeItem('neo.pendingActivation');
    } catch (e) {}
    var cm = document.getElementById('changes-modal');
    if (cm) cm.close();
    d.showModal();
  };
  window.confirmActivationReload = function () {
    try {
      localStorage.removeItem('neo.pendingActivation');
    } catch (e) {}
    var as = document.getElementById('activation-success');
    var cm = document.getElementById('changes-modal');
    if (as) as.close();
    if (cm) cm.close();
    window.location.reload();
  };

  // Live logs dialog helpers (EventSource for SSE from /sse/logs/<unit>)
  window.currentLogSource = null;
  window.openUnitLogs = function (unit) {
    var dlg = document.getElementById('logs-dialog');
    var nameEl = document.getElementById('log-unit');
    var pre = document.getElementById('log-pre');
    if (!dlg || !nameEl || !pre) return;
    nameEl.textContent = unit;
    pre.textContent = 'connecting live logs...\n';
    dlg.showModal();
    if (window.currentLogSource) {
      try {
        window.currentLogSource.close();
      } catch (e) {}
    }
    var es = new EventSource('/sse/logs/' + encodeURIComponent(unit));
    window.currentLogSource = es;
    es.onmessage = function (e) {
      pre.textContent += e.data + '\n';
      // auto-scroll if near bottom
      if (pre.scrollHeight - pre.scrollTop <= pre.clientHeight + 40) {
        pre.scrollTop = pre.scrollHeight;
      }
    };
    es.onerror = function () {
      pre.textContent += '[live connection hiccup; may resume]\n';
    };
    dlg.dataset.unit = unit;
  };
  window.closeUnitLogs = function () {
    var dlg = document.getElementById('logs-dialog');
    if (window.currentLogSource) {
      try {
        window.currentLogSource.close();
        window.currentLogSource = null;
      } catch (e) {}
    }
    if (dlg) dlg.close();
  };
  window.clearUnitLogs = function () {
    var pre = document.getElementById('log-pre');
    if (pre) pre.textContent = '';
  };
  var ld = document.getElementById('logs-dialog');
  if (ld) {
    ld.addEventListener('close', function () {
      if (window.currentLogSource) {
        try {
          window.currentLogSource.close();
          window.currentLogSource = null;
        } catch (e) {}
      }
    });
  }

  // Suppress noisy htmx swapErrors for OOB unit-controls updates when the row
  // is no longer in the DOM (e.g. switched panes, closed dialog, or rapid actions).
  // The status will be correct again on next pane load (which does a fresh bootstrap GET).
  document.body.addEventListener('htmx:swapError', function (evt) {
    try {
      var resp = evt.detail && evt.detail.xhr && evt.detail.xhr.responseText;
      if (resp && (/id="unit-controls-/.test(resp) || /id="update-out-/.test(resp))) {
        evt.stopImmediatePropagation();
      }
    } catch (e) {}
  });

  // Live unit status over /ws/status:
  // When an option pane with runtime units is shown, register those units with the
  // server so it polls systemctl ActiveState while this WebSocket stays open and
  // pushes OOB control HTML on every change. Leaving the pane (or disconnect)
  // clears interest so we do not poll forever.
  window.neoWsWrapper = null;
  window.neoWatchedUnits = [];

  function neoSendUnitWatch(op, units) {
    if (!window.neoWsWrapper || !units || !units.length) return;
    try {
      window.neoWsWrapper.send(JSON.stringify({ op: op, units: units }));
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
    window.neoWatchedUnits = next;
  }

  document.body.addEventListener('htmx:wsOpen', function (evt) {
    try {
      window.neoWsWrapper = evt.detail && evt.detail.socketWrapper;
      // Re-register after reconnect (htmx ws extension reconnects automatically).
      if (window.neoWatchedUnits && window.neoWatchedUnits.length) {
        neoSendUnitWatch('watch_replace', window.neoWatchedUnits);
      } else {
        neoSyncWatchedUnits(document.getElementById('config-content'));
      }
    } catch (e) {}
  });

  document.body.addEventListener('htmx:wsClose', function () {
    window.neoWsWrapper = null;
  });

  document.body.addEventListener('htmx:afterSwap', function (evt) {
    try {
      var t = evt.detail && evt.detail.target;
      if (!t) return;
      // Option pane (or grid without units) landed in #config-content.
      if (
        t.id === 'config-content' ||
        (t.closest && t.closest('#config-content') && t.id === 'options-pane')
      ) {
        neoSyncWatchedUnits(document.getElementById('config-content'));
      }
    } catch (e) {}
  });

  // If the option pane was already present when the script ran (unlikely), sync once.
  document.addEventListener('DOMContentLoaded', function () {
    neoSyncWatchedUnits(document.getElementById('config-content'));
  });
})();
