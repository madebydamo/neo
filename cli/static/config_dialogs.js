// config_dialogs.js
// Dialogs on the config page that are not tied to one pane: settings.toml download and
// upload, live unit logs, the option helper form. Runs after their markup is parsed.

/**
 * Download working-tree settings.toml without navigating this document.
 * A plain <a href="/settings/download"> would replace the page when this UI
 * is embedded in the navigator iframe (or when the attachment is blocked).
 */
window.neoDownloadSettingsToml = function neoDownloadSettingsToml() {
  var status = document.getElementById('settings-toml-status');
  fetch('/settings/download')
    .then(function (res) {
      if (!res.ok) {
        throw new Error(res.status === 404 ? 'settings.toml not found' : 'Download failed');
      }
      return res.blob();
    })
    .then(function (blob) {
      var url = URL.createObjectURL(blob);
      var a = document.createElement('a');
      a.href = url;
      a.download = 'settings.toml';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      setTimeout(function () {
        URL.revokeObjectURL(url);
      }, 1000);
    })
    .catch(function (err) {
      neoToast('Download failed: ' + err, 'error');
      if (status) {
        status.innerHTML = '<div class="alert alert-error text-sm">Download failed</div>';
      }
    });
};

/**
 * Replace working-tree settings.toml from a file input (Settings tab).
 * @param {HTMLInputElement} input
 */
window.neoUploadSettingsToml = function neoUploadSettingsToml(input) {
  var file = input.files && input.files[0];
  input.value = '';
  if (!file) return;
  var ok = window.confirm(
    'Replace working-tree settings.toml with "' +
      file.name +
      '"? Review pending changes and apply to activate.'
  );
  if (!ok) return;
  var status = document.getElementById('settings-toml-status');
  file
    .text()
    .then(function (text) {
      return fetch('/settings/upload', {
        method: 'POST',
        headers: { 'Content-Type': 'text/plain; charset=utf-8' },
        body: text,
      });
    })
    .then(function (res) {
      // 409: refused by an operation lock (toast, nothing to show in the card).
      if (res.status === 409 && window.NeoLocks) {
        return window.NeoLocks.handleFetch(res).then(function () {
          return null;
        });
      }
      return res.text();
    })
    .then(function (html) {
      if (html != null && status) status.innerHTML = html;
    })
    .catch(function (err) {
      neoToast('Upload failed: ' + err, 'error');
      if (status) {
        status.innerHTML = '<div class="alert alert-error text-sm">Upload failed</div>';
      }
    });
};

(function () {
  // Live unit logs (SSE from /sse/logs/<unit>). The output panel is the same
  // logPane activation and update use, including Latest when follow is paused.
  var currentLogSource = null;
  var unitLogsPane = null;

  function setLogConn(state) {
    var el = document.getElementById('log-conn');
    if (!el) return;
    var labels = { connecting: 'Connecting…', live: 'Live', reconnecting: 'Reconnecting…' };
    el.setAttribute('data-conn', state);
    el.textContent = labels[state] || '';
  }

  function ensureUnitLogsPane() {
    if (unitLogsPane) return unitLogsPane;
    var host = document.getElementById('logs-output');
    var api = window.NeoOpMonitor && window.NeoOpMonitor.logPane;
    if (!host || !api) return null;
    host.innerHTML = api.markup({ clear: true });
    unitLogsPane = api.bind(host.querySelector('.neo-op-output'), window);
    return unitLogsPane;
  }

  window.openUnitLogs = function (unit) {
    var dlg = document.getElementById('logs-dialog');
    var nameEl = document.getElementById('log-unit');
    var pane = ensureUnitLogsPane();
    if (!dlg || !nameEl || !pane) return;
    nameEl.textContent = unit;
    pane.clear();
    setLogConn('connecting');
    stopUnitLogs();
    dlg.showModal();
    pane.append('connecting live logs...\n');
    var es = new EventSource('/sse/logs/' + encodeURIComponent(unit));
    currentLogSource = es;
    es.onopen = function () { setLogConn('live'); };
    es.onmessage = function (e) {
      pane.append(e.data + '\n');
    };
    es.onerror = function () {
      setLogConn('reconnecting');
      pane.append('[live connection hiccup; may resume]\n');
    };
    dlg.dataset.unit = unit;
  };
  function stopUnitLogs() {
    if (!currentLogSource) return;
    var prev = currentLogSource;
    var currentLogSource = null;
    prev.onopen = null;
    prev.onmessage = null;
    prev.onerror = null;
    try { prev.close(); } catch (e) {}
  }
  var ld = document.getElementById('logs-dialog');
  if (ld) {
    ld.addEventListener('close', stopUnitLogs);
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
      wrap.className = 'flex flex-col w-full';
      var lab = document.createElement('label');
      lab.className = 'label py-1';
      // Helpers may come from plugins: the label is text, never markup.
      var labText = document.createElement('span');
      labText.className = 'label-text text-sm';
      labText.textContent = inp.label || inp.name;
      if (inp.required !== false) {
        var mark = document.createElement('span');
        mark.className = 'text-error';
        mark.textContent = ' *';
        labText.appendChild(mark);
      }
      lab.appendChild(labText);
      wrap.appendChild(lab);
      var el;
      if (inp.type === 'bool') {
        el = document.createElement('input');
        el.type = 'checkbox';
        el.className = 'toggle toggle-primary toggle-sm';
        el.checked = !!inp.default;
      } else if (inp.type === 'enum' && inp.values && inp.values.length) {
        el = document.createElement('select');
        el.className = 'select select-sm w-full font-mono';
        inp.values.forEach(function (v) {
          var o = document.createElement('option');
          o.value = v;
          o.textContent = v;
          el.appendChild(o);
        });
      } else {
        el = document.createElement('input');
        el.type = inp.type === 'password' ? 'password' : (inp.type === 'int' ? 'number' : 'text');
        el.className = 'input input-sm w-full font-mono';
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
})();
