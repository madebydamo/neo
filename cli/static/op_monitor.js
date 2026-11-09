// Live monitor for background ops (activation / update / store repair / generation switch).
//
// logPane is the shared output panel. Unit logs bind the same pane, so follow,
// Latest, copy, and trimming stay identical.
//
// The server renders only a mount point:
//   <div data-op-monitor="<id>" data-op-title="Activation" data-op-subtitle="…" [data-op-note="…"]>
// wherever it lands (#changes-body via htmx, versioning openModal, …). This script builds
// the monitor DOM once and keeps it in place; /ws/op/<id> streams state changes and
// appended log bytes. Nothing is re-rendered, so scroll position and text selection in
// the output survive new lines.
//
// Reconnects with backoff after a drop (neo-web restarts during a switch) and resumes
// the log from the last byte offset — no duplicated or missing lines.
(function (root, factory) {
  var api = factory((root && root.NeoUtil) || require('./neo_util.js'));
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root && root.document) {
    root.NeoOpMonitor = api;
    api.autostart(root);
  }
})(typeof window !== 'undefined' ? window : null, function (NeoUtil) {
  'use strict';

  var esc = NeoUtil.esc;

  var RESUME_KEY = 'neo.opMonitor';
  var BACKOFF_MS = [300, 700, 1500, 2500, 4000];
  /** Output kept in the DOM; older text is dropped from the top. */
  var MAX_CHARS = 1500000;

  function backoff(attempt) {
    return BACKOFF_MS[Math.min(attempt, BACKOFF_MS.length - 1)];
  }

  function wsUrl(loc, id, offset) {
    var proto = loc.protocol === 'https:' ? 'wss:' : 'ws:';
    return proto + '//' + loc.host + '/ws/op/' + encodeURIComponent(id) + '?offset=' + (offset || 0);
  }

  function humanPhase(phase) {
    return String(phase || '').replace(/[-_]+/g, ' ').trim();
  }

  /** Header badge for an op status. */
  function statusView(status) {
    switch (status) {
      case 'in_progress':
        return { label: 'Running', tone: 'running' };
      case 'success':
        return { label: 'Succeeded', tone: 'success' };
      case 'failed':
        return { label: 'Failed', tone: 'error' };
      case 'missing':
        return { label: 'Not found', tone: 'muted' };
      default:
        return { label: status ? humanPhase(status) : 'Connecting', tone: 'muted' };
    }
  }

  /** Per-step state for the stepper: done | current | failed | pending. */
  function stepStates(n, step, status) {
    var out = [];
    for (var i = 0; i < n; i++) {
      if (status === 'success' || i < step) out.push('done');
      else if (i === step && status === 'failed') out.push('failed');
      else if (i === step && status === 'in_progress') out.push('current');
      else out.push('pending');
    }
    return out;
  }

  /** Footer message + actions once the op is terminal. */
  function outcome(state) {
    var kind = state.kind;
    if (state.status === 'success') {
      if (kind === 'activation') {
        return {
          tone: 'success',
          text: 'Activated' + (state.branch ? ' as ' + state.branch : '') + '. Reload to use the new configuration.',
          reload: true,
        };
      }
      if (kind === 'update') return { tone: 'success', text: 'Inputs updated. Review the changes and activate to apply them.' };
      if (kind === 'repair') return { tone: 'success', text: 'Store repaired. Reload to re-evaluate the configuration.', reload: true };
      if (kind === 'genswitch') return { tone: 'success', text: 'Generation ' + (state.generation || '') + ' ' + (state.mode === 'boot' ? 'set as boot default.' : 'is active.'), reload: true };
      return { tone: 'success', text: 'Done.' };
    }
    if (state.status === 'failed') {
      return { tone: 'error', text: state.error || 'The operation failed — see the output above.' };
    }
    if (state.status === 'missing') {
      return { tone: 'muted', text: 'This operation is no longer on record (it may have been cleaned up).' };
    }
    return null;
  }

  var ICON_CHECK = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5" aria-hidden="true"><path fill-rule="evenodd" d="M16.7 5.3a1 1 0 010 1.4l-7.5 7.5a1 1 0 01-1.4 0l-3.5-3.5a1 1 0 111.4-1.4l2.8 2.8 6.8-6.8a1 1 0 011.4 0z" clip-rule="evenodd"/></svg>';
  var ICON_X = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5" aria-hidden="true"><path d="M6.3 5.2a.75.75 0 10-1.1 1.1L8.9 10l-3.7 3.7a.75.75 0 101.1 1.1L10 11.1l3.7 3.7a.75.75 0 101.1-1.1L11.1 10l3.7-3.7a.75.75 0 00-1.1-1.1L10 8.9 6.3 5.2z"/></svg>';
  var ICON_DOWN = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-3.5 h-3.5" aria-hidden="true"><path fill-rule="evenodd" d="M10 3a.75.75 0 01.75.75v10.5l3.97-3.97a.75.75 0 111.06 1.06l-5.25 5.25a.75.75 0 01-1.06 0l-5.25-5.25a.75.75 0 111.06-1.06l3.97 3.97V3.75A.75.75 0 0110 3z" clip-rule="evenodd"/></svg>';

  /** Slack before the tail counts as "not following". Shared with unit logs. */
  var NEAR_BOTTOM_PX = 24;

  /**
   * Terminal output panel used by activation / update and by unit logs.
   * Follows the tail until the reader scrolls away, then shows Latest.
   */
  var logPane = {
    nearBottom: function (el) {
      return el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM_PX;
    },

    /** `opts.clear` adds the Clear action unit logs use. */
    markup: function (opts) {
      opts = opts || {};
      var clearBtn = opts.clear
        ? '<button type="button" class="btn btn-ghost btn-xs" data-act="clear" title="Clear the output">Clear</button>'
        : '';
      return (
        '<div class="neo-op-output">' +
        '  <div class="neo-op-output-bar">' +
        '    <span class="font-medium">Output</span>' +
        '    <span class="tabular-nums opacity-60" data-el="lines"></span>' +
        '    <span class="opacity-60 hidden" data-el="trimmed">· earlier output hidden</span>' +
        '    <span class="flex-1"></span>' +
        clearBtn +
        '    <button type="button" class="btn btn-ghost btn-xs" data-act="copy" title="Copy the output">Copy</button>' +
        '  </div>' +
        '  <div class="neo-op-log-wrap">' +
        '    <pre class="neo-op-log" data-el="log" tabindex="0" aria-live="off"></pre>' +
        '    <button type="button" class="neo-op-jump btn btn-sm btn-neutral shadow-lg gap-1" data-act="jump" hidden>' + ICON_DOWN + 'Latest</button>' +
        '  </div>' +
        '</div>'
      );
    },

    bind: function (root, win, opts) {
      opts = opts || {};
      var doc = win.document;
      var maxChars = opts.maxChars || MAX_CHARS;
      var log = root.querySelector('[data-el="log"]');
      var linesEl = root.querySelector('[data-el="lines"]');
      var trimmedEl = root.querySelector('[data-el="trimmed"]');
      var jumpBtn = root.querySelector('[data-act="jump"]');
      var copyBtn = root.querySelector('[data-act="copy"]');
      var clearBtn = root.querySelector('[data-act="clear"]');
      var chars = 0;
      var lines = 0;
      var follow = true;

      function selectionInLog() {
        try {
          var sel = win.getSelection && win.getSelection();
          return !!(sel && !sel.isCollapsed && sel.anchorNode && log.contains(sel.anchorNode));
        } catch (e) {
          return false;
        }
      }

      log.addEventListener('scroll', function () {
        follow = logPane.nearBottom(log);
        jumpBtn.hidden = follow;
      }, { passive: true });
      jumpBtn.addEventListener('click', function () {
        follow = true;
        jumpBtn.hidden = true;
        log.scrollTop = log.scrollHeight;
      });
      if (copyBtn) {
        copyBtn.addEventListener('click', function (e) {
          var btn = e.currentTarget;
          var text = log.textContent || '';
          var done = function () {
            btn.textContent = 'Copied';
            if (win.setTimeout) win.setTimeout(function () { btn.textContent = 'Copy'; }, 1400);
          };
          try {
            win.navigator.clipboard.writeText(text).then(done, function () {});
          } catch (err) {}
        });
      }

      function discard() {
        log.textContent = '';
        chars = 0;
        lines = 0;
        linesEl.textContent = '';
      }

      function clear() {
        discard();
        trimmedEl.classList.add('hidden');
        follow = true;
        jumpBtn.hidden = true;
      }

      if (clearBtn) clearBtn.addEventListener('click', clear);

      return {
        log: log,
        append: function (text, truncated) {
          if (!text) return;
          if (truncated) trimmedEl.classList.remove('hidden');
          log.appendChild(doc.createTextNode(text));
          chars += text.length;
          lines += (String(text).match(/\n/g) || []).length;
          while (chars > maxChars && log.firstChild && log.firstChild !== log.lastChild) {
            chars -= log.firstChild.textContent.length;
            log.removeChild(log.firstChild);
            trimmedEl.classList.remove('hidden');
          }
          linesEl.textContent = lines + (lines === 1 ? ' line' : ' lines');
          if (follow && !selectionInLog()) log.scrollTop = log.scrollHeight;
        },
        discard: discard,
        clear: clear,
      };
    },
  };

  // ---------------------------------------------------------------------------
  // DOM monitor
  // ---------------------------------------------------------------------------

  function Monitor(el, win) {
    this.el = el;
    this.win = win;
    this.id = el.getAttribute('data-op-monitor');
    this.title = el.getAttribute('data-op-title') || 'Operation';
    this.subtitle = el.getAttribute('data-op-subtitle') || '';
    this.note = el.getAttribute('data-op-note') || '';
    this.state = { status: '', steps: [] };
    this.offset = 0;
    this.ended = false;
    this.disposed = false;
    this.attempt = 0;
    this.ws = null;
    this.retryTimer = null;
    this.stepsKey = '';
    this.build();
    this.setTitle();
    this.connect();
  }

  Monitor.prototype.build = function () {
    var el = this.el;
    el.setAttribute('data-op-mounted', '');
    el.classList.add('neo-op');
    el.innerHTML =
      '<div class="neo-op-chrome">' +
      (this.note
        ? '<div role="note" class="alert alert-soft alert-info text-sm py-2 px-3 mb-3">' + esc(this.note) + '</div>'
        : '') +
      '<div class="flex items-center gap-2 flex-wrap mb-3">' +
      '  <span class="neo-op-badge" data-tone="muted" data-el="badge"></span>' +
      '  <span class="text-xs font-mono text-base-content/60 truncate min-w-0" data-el="phase"></span>' +
      '  <span class="neo-op-conn ml-auto" data-conn="connecting" data-el="conn"></span>' +
      '</div>' +
      '<ol class="neo-op-steps mb-3" data-el="steps" hidden></ol>' +
      '</div>' +
      logPane.markup() +
      '<div class="neo-op-outcome hidden" data-el="outcome"></div>';

    var q = function (name) {
      return el.querySelector('[data-el="' + name + '"]');
    };
    this.badge = q('badge');
    this.phaseEl = q('phase');
    this.connEl = q('conn');
    this.stepsEl = q('steps');
    this.outcomeEl = q('outcome');
    this.pane = logPane.bind(el.querySelector('.neo-op-output'), this.win);
    this.renderState();
    this.setConn('connecting');
  };

  Monitor.prototype.setTitle = function () {
    var dlg = this.el.closest ? this.el.closest('dialog') : null;
    var h = dlg && dlg.querySelector('#changes-modal-title');
    if (!h) return;
    h.innerHTML = esc(this.title) +
      (this.subtitle ? ' <span class="font-normal text-sm opacity-50">' + esc(this.subtitle) + '</span>' : '');
  };

  Monitor.prototype.setConn = function (s) {
    var labels = { connecting: 'Connecting…', live: 'Live', reconnecting: 'Reconnecting…', done: '' };
    this.connEl.setAttribute('data-conn', s);
    this.connEl.textContent = labels[s] || '';
    this.connEl.title = s === 'reconnecting'
      ? 'Connection to the web UI lost (it restarts during a switch). The operation keeps running; output resumes automatically.'
      : '';
  };

  Monitor.prototype.renderState = function () {
    var st = this.state;
    var v = statusView(st.status);
    this.badge.setAttribute('data-tone', v.tone);
    this.badge.innerHTML =
      (v.tone === 'running' ? '<span class="loading loading-spinner loading-xs"></span>' : '') +
      (v.tone === 'success' ? ICON_CHECK : '') +
      (v.tone === 'error' ? ICON_X : '') +
      esc(v.label);
    this.phaseEl.textContent = st.status === 'in_progress' ? humanPhase(st.phase) : '';

    var steps = st.steps || [];
    this.stepsEl.hidden = !steps.length;
    if (steps.length) {
      var key = steps.join('|');
      if (key !== this.stepsKey) {
        // Built once; later updates only flip data-state so animations keep running.
        this.stepsKey = key;
        this.stepsEl.innerHTML = steps.map(function (label, i) {
          return '<li class="neo-op-step" data-state="pending"><span class="neo-op-step-dot">' +
            '<span class="neo-op-step-num">' + (i + 1) + '</span></span>' +
            '<span class="neo-op-step-label">' + esc(label) + '</span></li>';
        }).join('');
      }
      var states = stepStates(steps.length, st.step || 0, st.status);
      var items = this.stepsEl.children;
      for (var i = 0; i < items.length; i++) {
        if (items[i].getAttribute('data-state') === states[i]) continue;
        items[i].setAttribute('data-state', states[i]);
        var dot = items[i].firstChild;
        if (states[i] === 'done') dot.innerHTML = ICON_CHECK;
        else if (states[i] === 'failed') dot.innerHTML = ICON_X;
        else if (states[i] === 'current') dot.innerHTML = '<span class="loading loading-spinner loading-xs"></span>';
        else dot.innerHTML = '<span class="neo-op-step-num">' + (i + 1) + '</span>';
      }
    }

    var out = outcome(st);
    if (!out) {
      this.outcomeEl.classList.add('hidden');
      this.outcomeEl.innerHTML = '';
      return;
    }
    this.outcomeEl.classList.remove('hidden');
    var tone = out.tone === 'success' ? 'alert-success' : out.tone === 'error' ? 'alert-error' : '';
    this.outcomeEl.innerHTML =
      '<div role="status" class="alert alert-soft ' + tone + ' text-sm py-2.5 px-3 flex flex-wrap items-center gap-3 w-full">' +
      '<span class="min-w-0 flex-1 whitespace-pre-wrap break-words">' + esc(out.text) + '</span>' +
      '<span class="flex gap-2 shrink-0 sm:ml-auto">' +
      (out.reload
        ? '<button type="button" class="btn btn-sm btn-success" data-act="reload">Reload</button>'
        : '<button type="button" class="btn btn-sm" data-act="close">Close</button>') +
      '</span></div>';
    var self = this;
    var reload = this.outcomeEl.querySelector('[data-act="reload"]');
    if (reload) reload.addEventListener('click', function () {
      forgetResume(self.win);
      self.win.location.reload();
    });
    var close = this.outcomeEl.querySelector('[data-act="close"]');
    if (close) close.addEventListener('click', function () {
      var dlg = self.el.closest('dialog');
      if (dlg) dlg.close();
    });
  };

  Monitor.prototype.append = function (text, truncated) {
    // Text nodes are appended, never replaced: selections and scroll stay put.
    this.pane.append(text, truncated);
  };

  Monitor.prototype.onMessage = function (msg) {
    switch (msg.t) {
      case 'state':
        this.state = msg;
        if (msg.title) this.title = msg.title;
        if (msg.status === 'in_progress') rememberResume(this.win, this.id, this.el);
        this.renderState();
        break;
      case 'log':
        if (typeof msg.from === 'number' && msg.from !== this.offset && this.offset !== 0 && !msg.truncated) {
          // Out of sync (should not happen): start clean from this chunk.
          this.pane.discard();
        }
        this.append(msg.data, msg.truncated);
        if (typeof msg.to === 'number') this.offset = msg.to;
        break;
      case 'reset':
        this.pane.discard();
        this.offset = 0;
        break;
      case 'end':
        this.ended = true;
        this.setConn('done');
        break;
      case 'error':
        this.ended = true;
        this.setConn('done');
        this.state = { status: 'failed', error: msg.message || 'error', steps: this.state.steps };
        this.renderState();
        break;
    }
  };

  Monitor.prototype.connect = function () {
    if (this.disposed || this.ended) return;
    var self = this;
    var ws;
    try {
      ws = new this.win.WebSocket(wsUrl(this.win.location, this.id, this.offset));
    } catch (e) {
      this.scheduleRetry();
      return;
    }
    this.ws = ws;
    ws.onopen = function () {
      self.attempt = 0;
      self.setConn('live');
    };
    ws.onmessage = function (e) {
      var msg;
      try { msg = JSON.parse(e.data); } catch (err) { return; }
      self.onMessage(msg);
    };
    ws.onclose = function () {
      if (self.ws === ws) self.ws = null;
      if (self.disposed || self.ended) return;
      self.setConn('reconnecting');
      self.scheduleRetry();
    };
  };

  Monitor.prototype.scheduleRetry = function () {
    var self = this;
    if (this.retryTimer) return;
    this.retryTimer = this.win.setTimeout(function () {
      self.retryTimer = null;
      self.connect();
    }, backoff(this.attempt++));
  };

  /** Skip the backoff wait (tab visible again / network back). */
  Monitor.prototype.poke = function () {
    if (this.disposed || this.ended || this.ws || !this.retryTimer) return;
    this.win.clearTimeout(this.retryTimer);
    this.retryTimer = null;
    this.connect();
  };

  Monitor.prototype.dispose = function () {
    this.disposed = true;
    if (this.retryTimer) this.win.clearTimeout(this.retryTimer);
    if (this.ws) {
      try { this.ws.close(); } catch (e) {}
    }
    this.ws = null;
  };

  // ---------------------------------------------------------------------------
  // Resume after a page reload (while the op is still running)
  // ---------------------------------------------------------------------------

  function rememberResume(win, id, el) {
    if (!el.closest || !el.closest('#changes-modal')) return;
    NeoUtil.store('local', win).set(RESUME_KEY, id);
  }

  function forgetResume(win) {
    NeoUtil.store('local', win).remove(RESUME_KEY);
  }

  function pendingResume(win) {
    return NeoUtil.store('local', win).get(RESUME_KEY);
  }

  // ---------------------------------------------------------------------------
  // Auto-mount
  // ---------------------------------------------------------------------------

  var live = [];

  function scan(win) {
    var doc = win.document;
    for (var i = live.length - 1; i >= 0; i--) {
      if (!live[i].el.isConnected) {
        live[i].dispose();
        live.splice(i, 1);
      }
    }
    var els = doc.querySelectorAll('[data-op-monitor]:not([data-op-mounted])');
    for (var j = 0; j < els.length; j++) live.push(new Monitor(els[j], win));
  }

  function autostart(win) {
    if (!win || !win.document || typeof win.MutationObserver !== 'function') return;
    var doc = win.document;
    var queued = false;
    var run = function () {
      queued = false;
      scan(win);
    };
    var start = function () {
      new win.MutationObserver(function () {
        if (queued) return;
        queued = true;
        win.queueMicrotask ? win.queueMicrotask(run) : win.setTimeout(run, 0);
      }).observe(doc.body, { childList: true, subtree: true });
      scan(win);
      // A closed dialog is a dismissed monitor: do not reopen it on the next load.
      var modal = doc.getElementById('changes-modal');
      // Closing it also stops streaming, even where the page keeps the dialog body.
      if (modal) modal.addEventListener('close', function () {
        forgetResume(win);
        for (var i = live.length - 1; i >= 0; i--) {
          if (modal.contains(live[i].el)) {
            live[i].dispose();
            live.splice(i, 1);
          }
        }
      });
    };
    NeoUtil.onReady(doc, start);
    doc.addEventListener('visibilitychange', function () {
      if (doc.visibilityState === 'visible') live.forEach(function (m) { m.poke(); });
    });
    win.addEventListener('online', function () { live.forEach(function (m) { m.poke(); }); });
  }

  /** Reopen the shared dialog on the op that was running before a reload. */
  function resume(win) {
    var id = pendingResume(win);
    if (!id || !/^[a-z]+_[A-Za-z0-9_-]+$/.test(id)) return;
    var modal = win.document.getElementById('changes-modal');
    var body = win.document.getElementById('changes-body');
    if (!modal || !body || typeof win.htmx === 'undefined') return;
    modal.showModal();
    win.htmx.ajax('GET', '/op/monitor/' + encodeURIComponent(id), { target: '#changes-body', swap: 'innerHTML' });
  }

  return {
    backoff: backoff,
    wsUrl: wsUrl,
    statusView: statusView,
    stepStates: stepStates,
    outcome: outcome,
    humanPhase: humanPhase,
    logPane: logPane,
    autostart: autostart,
    resume: resume,
    Monitor: Monitor,
  };
});
