// configuration.js
// Client helpers for the homeserver config page: activation resume, live unit logs (SSE),
// WebSocket unit-watch registration, and the Alpine config shell (tabs + breadcrumb).
// Loaded only by configuration.html.hbs.

/**
 * Show a transient DaisyUI alert toast outside #config-content.
 * @param {string} message
 * @param {'success'|'error'|'info'|'warning'} [type]
 */
window.neoToast = function neoToast(message, type) {
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
};

/**
 * True when #options-pane exists and any option field differs from its original value.
 * Uses Alpine.$data(pane) (optionForm: values / originals / isAtOriginal).
 */
window.neoIsConfigFormDirty = function neoIsConfigFormDirty() {
  try {
    var pane = document.getElementById('options-pane');
    if (!pane) return false;
    if (typeof Alpine === 'undefined' || !Alpine.$data) return false;
    var data = Alpine.$data(pane);
    if (!data || !data.values || typeof data.isAtOriginal !== 'function') return false;
    var names = Object.keys(data.values);
    for (var i = 0; i < names.length; i++) {
      if (!data.isAtOriginal(names[i])) return true;
    }
    return false;
  } catch (e) {
    return false;
  }
};

/**
 * If the option form is dirty, ask the user to discard unsaved changes.
 * Returns true when navigation may proceed (not dirty, or user confirmed discard).
 */
window.neoConfirmLeaveConfigForm = function neoConfirmLeaveConfigForm() {
  if (!window.neoIsConfigFormDirty()) return true;
  return window.confirm('You have unsaved changes. Discard them?');
};

// One-shot: loadTab/goUp confirm first, then set this so htmx:beforeRequest does not re-prompt.
window.neoAllowNextConfigNav = false;

/** Alpine shell for the config page: tabs, detail breadcrumb, and up navigation. */
window.configShell = function configShell() {
  var el = document.getElementById('config-shell');
  var initialTab = (el && el.getAttribute('data-initial-tab')) || 'services';
  var initialDetail = el && el.getAttribute('data-initial-detail');
  if (initialDetail === '') initialDetail = null;
  return {
    tab: initialTab, // 'services' | 'settings' | 'versioning'
    detail: initialDetail || null, // null | service/core section name when option pane is open

    sectionLabel() {
      if (this.tab === 'settings') return 'Settings';
      if (this.tab === 'versioning') return 'Versioning';
      return 'Services';
    },

    setDetail(name) {
      this.detail = name || null;
    },

    clearDetail() {
      this.detail = null;
    },

    /** Canonical tab content URL under /configuration/... */
    tabUrl(tab) {
      if (tab === 'settings') return '/configuration/settings';
      if (tab === 'versioning') return '/configuration/versioning';
      // Prefer /configuration (not /configuration/services) as the services home URL.
      return '/configuration';
    },

    /**
     * Navigate #config-content and push the URL into history.
     *
     * htmx 1.9 `htmx.ajax(..., { push })` is ignored — history only comes from
     * `hx-push-url` on the *source* element. Pass #config-nav-helper as source
     * (it has hx-push-url="true") so the request path is pushed like card nav.
     */
    navigateContent(url) {
      if (typeof htmx === 'undefined' || !htmx.ajax) return;
      window.neoAllowNextConfigNav = true;
      var helper = document.getElementById('config-nav-helper');
      var opts = {
        target: '#config-content',
        swap: 'innerHTML',
      };
      if (helper) {
        opts.source = helper;
      }
      var p = htmx.ajax('GET', url, opts);
      // If helper is missing, still update the location bar after the swap.
      if (!helper && p && typeof p.then === 'function') {
        p.then(function () {
          try {
            if (location.pathname + location.search !== url) {
              history.pushState({ htmx: true }, '', url);
            }
          } catch (e) {}
        });
      }
    },

    /**
     * Switch tab and load the corresponding grid/branches into #config-content.
     * Do not clear `detail` here — breadcrumb chrome stays until afterSwap so the
     * layout does not collapse while the option pane is still on screen.
     */
    loadTab(tab) {
      if (!window.neoConfirmLeaveConfigForm()) return;
      this.tab = tab;
      this.navigateContent(this.tabUrl(tab));
    },

    /**
     * Leave the option pane and return to the parent grid for the current tab.
     * Breadcrumb stays visible until the grid swap lands (see syncConfigShellFromContent).
     */
    goUp() {
      if (!window.neoConfirmLeaveConfigForm()) return;
      this.navigateContent(this.tabUrl(this.tab));
    },

    /** Logo / home: services tab at /configuration. */
    goHome() {
      if (!window.neoConfirmLeaveConfigForm()) return;
      this.tab = 'services';
      this.navigateContent('/configuration');
    },
  };
};

(function () {
  /** Instant navbar spinner (no CSS fade — ms requests must not feel like 200ms). */
  function setNavBusy(on) {
    var el = document.getElementById('nav-busy');
    if (!el) return;
    el.classList.toggle('is-busy', !!on);
    el.setAttribute('aria-hidden', on ? 'false' : 'true');
  }

  /** Resolve #config-content whether detail.target is an element or a selector string. */
  function isConfigContentTarget(target) {
    if (!target) return false;
    if (typeof target === 'string') {
      return target === '#config-content' || target === 'config-content';
    }
    if (target.id === 'config-content') return true;
    // htmx sometimes reports the requesting elt; treat swaps into #config-content as nav.
    return !!(target.getAttribute && target.getAttribute('hx-target') === '#config-content');
  }

  /**
   * Hold #config-content's current height while a nav request is in flight so the
   * document does not collapse when a tall option pane is swapped for a shorter grid
   * (or while waiting for the response). Cleared after the swap settles.
   */
  function lockConfigContentHeight() {
    var el = document.getElementById('config-content');
    if (!el) return;
    var h = el.offsetHeight;
    if (h > 0) el.style.minHeight = h + 'px';
  }

  function unlockConfigContentHeight() {
    var el = document.getElementById('config-content');
    if (!el) return;
    el.style.minHeight = '';
  }

  // Guard attribute-based (and other) HTMX navigations that replace #config-content.
  // Skip modals, action bar, unit controls, changes-body, etc. (other targets).
  // After a successful save, option_form updates originals before reload, so dirty is false.
  document.body.addEventListener('htmx:beforeRequest', function (evt) {
    try {
      var d = evt.detail || {};
      var t = d.target;
      var isNav = isConfigContentTarget(t) || isConfigContentTarget(d.elt);
      if (!isNav && d.requestConfig && d.requestConfig.target) {
        isNav = isConfigContentTarget(d.requestConfig.target);
      }
      if (!isNav) return;
      setNavBusy(true);
      lockConfigContentHeight();
      if (window.neoAllowNextConfigNav) {
        window.neoAllowNextConfigNav = false;
        return;
      }
      if (!window.neoConfirmLeaveConfigForm()) {
        setNavBusy(false);
        unlockConfigContentHeight();
        evt.preventDefault();
      }
    } catch (e) {}
  });

  // Always clear nav spinner when a config-content request finishes (or errors / aborts).
  function clearNavBusyIfConfig(evt) {
    try {
      var d = evt.detail || {};
      var t = d.target;
      var isNav = isConfigContentTarget(t) || isConfigContentTarget(d.elt);
      if (!isNav && d.requestConfig && d.requestConfig.target) {
        isNav = isConfigContentTarget(d.requestConfig.target);
      }
      if (!isNav) return;
      setNavBusy(false);
    } catch (e) {
      setNavBusy(false);
    }
  }
  document.body.addEventListener('htmx:afterRequest', clearNavBusyIfConfig);
  document.body.addEventListener('htmx:afterSwap', clearNavBusyIfConfig);
  document.body.addEventListener('htmx:responseError', function (evt) {
    clearNavBusyIfConfig(evt);
    try {
      if (isConfigContentTarget((evt.detail || {}).target)) unlockConfigContentHeight();
    } catch (e) {
      unlockConfigContentHeight();
    }
  });
  document.body.addEventListener('htmx:sendError', function (evt) {
    clearNavBusyIfConfig(evt);
    try {
      if (isConfigContentTarget((evt.detail || {}).target)) unlockConfigContentHeight();
    } catch (e) {
      unlockConfigContentHeight();
    }
  });

  // Browser leave / hard navigation while the option form has unsaved edits.
  window.addEventListener('beforeunload', function (e) {
    try {
      if (!window.neoIsConfigFormDirty()) return;
      e.preventDefault();
      e.returnValue = '';
    } catch (err) {}
  });

  /** Sync shell.detail / shell.tab after HTMX swaps into #config-content. */
  function syncConfigShellFromContent() {
    try {
      var shell = document.getElementById('config-shell');
      if (!shell || typeof Alpine === 'undefined' || !Alpine.$data) return;
      var data = Alpine.$data(shell);
      if (!data) return;
      var content = document.getElementById('config-content');
      var pane = content && content.querySelector('#options-pane');
      if (pane) {
        data.setDetail(pane.getAttribute('data-service'));
        data.tab = pane.getAttribute('data-is-core') === 'true' ? 'settings' : 'services';
      } else {
        data.clearDetail();
        // Infer active tab from grid/branches partials (Back/Forward history restore).
        if (content && content.querySelector('#core-grid')) data.tab = 'settings';
        else if (content && content.querySelector('#branches-section')) data.tab = 'versioning';
        else if (content && content.querySelector('#services-grid')) data.tab = 'services';
      }
    } catch (e) {}
  }

  /** Scroll the config page (and parent iframe, if embedded) back to the top. */
  function resetConfigViewport() {
    try {
      window.scrollTo(0, 0);
      if (document.documentElement) document.documentElement.scrollTop = 0;
      if (document.body) document.body.scrollTop = 0;
      // When neo config is shown inside the navigator iframe, also reset the outer frame.
      try {
        if (window.parent && window.parent !== window) {
          window.parent.scrollTo(0, 0);
        }
      } catch (e) {}
    } catch (e) {}
  }

  // Sync chrome as soon as content lands (afterSwap), not afterSettle — so the
  // breadcrumb is updated in the same paint cycle as the pane/grid swap.
  // Then release the height lock and scroll on the next frame so Alpine can
  // apply detail chrome before the document reflows (one layout change, not two).
  document.body.addEventListener('htmx:afterSwap', function (evt) {
    try {
      var t = evt.detail && evt.detail.target;
      if (!t) return;
      if (
        t.id === 'config-content' ||
        (t.closest && t.closest('#config-content') && t.id === 'options-pane')
      ) {
        syncConfigShellFromContent();
        requestAnimationFrame(function () {
          unlockConfigContentHeight();
          // Opening/closing the option pane (or switching tabs) replaces the main
          // content while preserving document scroll — always start at the top.
          resetConfigViewport();
        });
      }
    } catch (e) {}
  });

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
  // Only on full page load — never on htmx:afterSettle (status/log polls settle too and
  // would re-fetch the monitor in a loop while neo.pendingActivation is set).
  document.addEventListener('DOMContentLoaded', resume);

  /** Log panel ids used by activation / update / repair monitors. */
  var MONITOR_LOG_IDS = { 'act-log': 1, 'update-log': 1, 'repair-log': 1 };

  /**
   * Tear down monitor HTMX polls. Closing the dialog used to leave #changes-body
   * with every-1s status/log elements still in the DOM (and still requesting).
   * Completion is already pushed via the action-bar WebSocket.
   */
  function clearChangesMonitor() {
    var body = document.getElementById('changes-body');
    if (!body) return;
    // Removing nodes cancels HTMX intervals bound to them.
    body.innerHTML = '';
  }

  var changesModal = document.getElementById('changes-modal');
  if (changesModal) {
    changesModal.addEventListener('close', clearChangesMonitor);
  }

  document.body.addEventListener('htmx:beforeSwap', function (e) {
    var t = e.detail && e.detail.target;
    if (t && MONITOR_LOG_IDS[t.id]) {
      t.dataset.atBottom =
        t.scrollTop + t.clientHeight >= t.scrollHeight - 8 ? '1' : '';
    }
  });
  document.body.addEventListener('htmx:afterSwap', function (e) {
    var t = e.detail && e.detail.target;
    if (t && MONITOR_LOG_IDS[t.id] && t.dataset.atBottom) {
      t.scrollTop = t.scrollHeight;
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
    // Never carry live hx polls into the success dialog.
    var hxEls = clone.querySelectorAll('[hx-get], [hx-trigger]');
    for (var j = 0; j < hxEls.length; j++) {
      hxEls[j].removeAttribute('hx-get');
      hxEls[j].removeAttribute('hx-trigger');
      hxEls[j].removeAttribute('hx-swap');
    }
    b.innerHTML = clone.innerHTML;
    try {
      localStorage.removeItem('neo.pendingActivation');
    } catch (e) {}
    var cm = document.getElementById('changes-modal');
    if (cm) cm.close(); // also clearChangesMonitor via close listener
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

  // Option helper form dialog (password/hash generators etc.)
  var helperDialogOnApply = null;
  window.openHelperDialog = function (helper, onApply) {
    var dlg = document.getElementById('helper-dialog');
    var title = document.getElementById('helper-dialog-title');
    var desc = document.getElementById('helper-dialog-desc');
    var fields = document.getElementById('helper-dialog-fields');
    var form = document.getElementById('helper-dialog-form');
    if (!dlg || !fields || !form) return;
    helperDialogOnApply = onApply;
    if (title) title.textContent = helper.label || 'Helper';
    if (desc) {
      desc.textContent = helper.description || '';
      desc.style.display = helper.description ? '' : 'none';
    }
    fields.innerHTML = '';
    (helper.inputs || []).forEach(function (inp) {
      var wrap = document.createElement('div');
      wrap.className = 'form-control w-full';
      var lab = document.createElement('label');
      lab.className = 'label py-1';
      lab.innerHTML = '<span class="label-text text-sm">' +
        (inp.label || inp.name) +
        (inp.required === false ? '' : ' <span class="text-error">*</span>') +
        '</span>';
      wrap.appendChild(lab);
      var el;
      if (inp.type === 'bool') {
        el = document.createElement('input');
        el.type = 'checkbox';
        el.className = 'toggle toggle-primary toggle-sm';
        el.checked = !!inp.default;
      } else if (inp.type === 'enum' && inp.values && inp.values.length) {
        el = document.createElement('select');
        el.className = 'select select-bordered select-sm w-full font-mono';
        inp.values.forEach(function (v) {
          var o = document.createElement('option');
          o.value = v;
          o.textContent = v;
          el.appendChild(o);
        });
      } else {
        el = document.createElement('input');
        el.type = inp.type === 'password' ? 'password' : (inp.type === 'int' ? 'number' : 'text');
        el.className = 'input input-bordered input-sm w-full font-mono';
        if (inp.placeholder) el.placeholder = inp.placeholder;
        if (inp.type === 'password') el.autocomplete = 'new-password';
        if (inp.default != null) el.value = String(inp.default);
      }
      el.name = inp.name;
      el.dataset.helperInput = inp.name;
      el.dataset.helperType = inp.type || 'str';
      el.required = inp.required !== false;
      wrap.appendChild(el);
      fields.appendChild(wrap);
    });
    form.onsubmit = function (ev) {
      ev.preventDefault();
      var inputs = {};
      var nodes = fields.querySelectorAll('[data-helper-input]');
      for (var i = 0; i < nodes.length; i++) {
        var node = nodes[i];
        var name = node.dataset.helperInput;
        var t = node.dataset.helperType || 'str';
        if (t === 'bool') {
          inputs[name] = !!node.checked;
        } else if (t === 'int') {
          inputs[name] = node.value === '' ? null : Number(node.value);
        } else {
          inputs[name] = node.value;
        }
      }
      var cb = helperDialogOnApply;
      window.closeHelperDialog();
      if (typeof cb === 'function') cb(inputs);
      return false;
    };
    dlg.showModal();
    var first = fields.querySelector('input, select');
    if (first) setTimeout(function () { first.focus(); }, 50);
  };
  window.closeHelperDialog = function () {
    var dlg = document.getElementById('helper-dialog');
    var fields = document.getElementById('helper-dialog-fields');
    helperDialogOnApply = null;
    if (fields) fields.innerHTML = '';
    if (dlg) dlg.close();
  };
  var hd = document.getElementById('helper-dialog');
  if (hd) {
    hd.addEventListener('close', function () {
      helperDialogOnApply = null;
      var fields = document.getElementById('helper-dialog-fields');
      if (fields) fields.innerHTML = '';
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
