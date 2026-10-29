// agent_chat.js — the assistant chat drawer (Hermes via /agent/*).
//
// NeoAgent.open(context, { title, starter }) opens the drawer for a context
// ("general", "service:<name>", "guide:<id>"), resumes that chat and, for a
// new chat with a starter, sends it. Turns stream Hermes' session SSE events
// (assistant.delta, tool.*, approval.request, assistant.completed, …).
// Assistant replies are rendered server-side (/agent/render, same Markdown and
// neo links as guides); /proposals/<id> links in a reply become the proposal
// card (Save / Save & apply / Reject).
(function (root) {
  'use strict';

  /** Split an SSE text buffer into complete frames; returns { events, rest }. */
  function parseSse(buffer) {
    var events = [];
    var parts = buffer.replace(/\r\n/g, '\n').split('\n\n');
    var rest = parts.pop();
    parts.forEach(function (block) {
      var name = 'message';
      var data = [];
      block.split('\n').forEach(function (line) {
        if (!line || line.charAt(0) === ':') return;
        var i = line.indexOf(':');
        var field = i < 0 ? line : line.slice(0, i);
        var value = i < 0 ? '' : line.slice(i + 1).replace(/^ /, '');
        if (field === 'event') name = value;
        else if (field === 'data') data.push(value);
      });
      if (!data.length) return;
      var payload;
      try {
        payload = JSON.parse(data.join('\n'));
      } catch (e) {
        payload = { text: data.join('\n') };
      }
      events.push({ event: name, data: payload });
    });
    return { events: events, rest: rest };
  }

  /** Proposal ids linked in a reply (/proposals/<16 hex>), in order, unique. */
  function proposalIds(text) {
    var out = [];
    var re = /\/proposals\/([0-9a-f]{16})\b/g;
    var m;
    while ((m = re.exec(text || ''))) {
      if (out.indexOf(m[1]) < 0) out.push(m[1]);
    }
    return out;
  }

  /** Short label for a tool event ("mcp_neo_neo_settings_get" → "Reading settings"). */
  function toolLabel(name) {
    var n = String(name || '');
    if (/settings_propose/.test(n)) return 'Preparing a settings change';
    if (/settings_(get|schema)/.test(n)) return 'Reading settings';
    if (/settings_status/.test(n)) return 'Checking the suggested change';
    if (/terminal|shell|exec/.test(n)) return 'Running a command';
    if (/skill/.test(n)) return 'Reading instructions';
    if (/web|browser|search/.test(n)) return 'Looking it up';
    if (n === '_thinking') return 'Thinking';
    return 'Working (' + n.replace(/^mcp_[a-z]+_/, '') + ')';
  }

  var api = { parseSse: parseSse, proposalIds: proposalIds, toolLabel: toolLabel };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  if (typeof document === 'undefined') return;

  var state = { context: 'general', session: null, run: null, busy: false, first: false, starter: null, hint: null };

  function el(id) {
    return document.getElementById(id);
  }

  function scrollDown() {
    var log = el('neo-agent-log');
    if (log) log.scrollTop = log.scrollHeight;
  }

  function bubble(role, html, text) {
    var log = el('neo-agent-log');
    var b = document.createElement('div');
    b.className = 'neo-agent-msg neo-agent-msg--' + role;
    if (html != null) b.innerHTML = html;
    else b.textContent = text || '';
    log.appendChild(b);
    scrollDown();
    return b;
  }

  function note(text, cls) {
    var n = bubble('note', null, text);
    if (cls) n.classList.add(cls);
    return n;
  }

  function post(url, body) {
    return fetch(url, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  }

  function renderInto(node, text) {
    return post('/agent/render', { text: text })
      .then(function (r) {
        return r.ok ? r.text() : null;
      })
      .then(function (html) {
        if (html != null) node.innerHTML = html;
        else node.textContent = text;
        if (typeof htmx !== 'undefined') htmx.process(node);
        proposalIds(text).forEach(function (id) {
          var slot = document.createElement('div');
          slot.className = 'mt-2';
          node.appendChild(slot);
          if (typeof htmx !== 'undefined') {
            htmx.ajax('GET', '/proposals/' + id + '/card', { target: slot, swap: 'innerHTML' });
          }
        });
        scrollDown();
      });
  }

  function approvalCard(d) {
    var card = bubble('approval', '');
    var title = document.createElement('div');
    title.className = 'font-medium text-sm';
    title.textContent = 'The assistant wants to do this:';
    card.appendChild(title);
    var what = document.createElement('div');
    what.className = 'text-sm mt-1';
    what.textContent = d.description || d.reason || '';
    if (what.textContent) card.appendChild(what);
    if (d.command) {
      var pre = document.createElement('pre');
      pre.className = 'neo-agent-cmd';
      pre.textContent = d.command;
      card.appendChild(pre);
    }
    var row = document.createElement('div');
    row.className = 'flex flex-wrap gap-2 mt-2';
    var labels = { once: 'Allow once', session: 'Allow for this chat', always: 'Always allow', deny: 'Deny' };
    (d.choices || ['once', 'deny']).forEach(function (c) {
      var btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'btn btn-xs ' + (c === 'deny' ? 'btn-ghost' : c === 'once' ? 'btn-primary' : '');
      btn.textContent = labels[c] || c;
      btn.addEventListener('click', function () {
        row.querySelectorAll('button').forEach(function (b) {
          b.disabled = true;
        });
        post('/agent/approve', { run_id: d.run_id || state.run, choice: c }).then(function (r) {
          var done = document.createElement('div');
          done.className = 'text-xs text-base-content/60 mt-1';
          done.textContent = r.ok ? (c === 'deny' ? 'Denied.' : 'Allowed.') : 'Could not send the answer.';
          card.appendChild(done);
        });
      });
      row.appendChild(btn);
    });
    card.appendChild(row);
  }

  function setBusy(on) {
    state.busy = on;
    var send = el('neo-agent-send');
    var stop = el('neo-agent-stop');
    if (send) send.classList.toggle('hidden', on);
    if (stop) stop.classList.toggle('hidden', !on);
  }

  /**
   * One turn as a timeline: text segments, tool groups and approval cards are
   * appended in the order their events arrive. Consecutive tool calls share a
   * collapsible group; a finished call marks the earliest running call with the
   * same name (calls can run in parallel and finish in any order).
   */
  function Turn(log) {
    this.log = log;
    this.seg = null; // last appended element of this turn
    this.texts = []; // { el, raw }
    this.lines = []; // { name, el, done }
    this.groups = [];
    this.streamed = '';
  }

  Turn.prototype.append = function (el) {
    this.log.appendChild(el);
    this.seg = el;
    scrollDown();
    return el;
  };

  Turn.prototype.text = function (delta) {
    var t = this.texts[this.texts.length - 1];
    if (!t || this.seg !== t.el) {
      var el = document.createElement('div');
      el.className = 'neo-agent-msg neo-agent-msg--assistant neo-agent-typing';
      t = { el: el, raw: '' };
      this.texts.push(t);
      this.append(el);
    }
    t.raw += delta;
    t.el.textContent = t.raw;
    scrollDown();
  };

  Turn.prototype.group = function () {
    var g = this.groups[this.groups.length - 1];
    if (g && this.seg === g.el) return g;
    var el = document.createElement('details');
    el.className = 'neo-agent-tools';
    var sum = document.createElement('summary');
    var body = document.createElement('div');
    body.className = 'neo-agent-tools-body';
    el.appendChild(sum);
    el.appendChild(body);
    g = { el: el, sum: sum, body: body, lines: [] };
    this.groups.push(g);
    this.append(el);
    return g;
  };

  Turn.prototype.summarize = function (g, running) {
    var failed = g.lines.filter(function (l) { return l.failed; }).length;
    var open = g.lines.filter(function (l) { return !l.done; });
    if (running && open.length) {
      g.sum.innerHTML = '<span class="loading loading-dots loading-xs"></span> ';
      g.sum.appendChild(document.createTextNode(toolLabel(open[open.length - 1].name)));
    } else {
      g.sum.textContent =
        (failed ? '⚠ ' : '✓ ') + 'Used ' + g.lines.length + (g.lines.length === 1 ? ' tool' : ' tools') +
        (failed ? ' (' + failed + ' failed)' : '');
    }
  };

  Turn.prototype.toolStarted = function (name) {
    var g = this.group();
    var el = document.createElement('div');
    el.textContent = '… ' + toolLabel(name);
    g.body.appendChild(el);
    var line = { name: name, el: el, done: false, failed: false, group: g };
    g.lines.push(line);
    this.lines.push(line);
    this.summarize(g, true);
    scrollDown();
  };

  Turn.prototype.toolDone = function (name, failed) {
    var line = null;
    for (var i = 0; i < this.lines.length; i++) {
      if (!this.lines[i].done && this.lines[i].name === name) { line = this.lines[i]; break; }
    }
    if (!line) return;
    line.done = true;
    line.failed = failed;
    line.el.textContent = (failed ? '✗ ' : '✓ ') + toolLabel(name);
    this.summarize(line.group, true);
  };

  Turn.prototype.finish = function (finalText) {
    var self = this;
    this.lines.forEach(function (l) {
      if (!l.done) { l.done = true; l.el.textContent = '– ' + toolLabel(l.name); }
    });
    this.groups.forEach(function (g) { self.summarize(g, false); });
    // The final answer arrives in full with assistant.completed; it is the
    // streamed text when there was any.
    if (!this.texts.length && finalText && finalText.trim()) this.text(finalText);
    var chain = Promise.resolve();
    this.texts.forEach(function (t) {
      t.el.classList.remove('neo-agent-typing');
      if (t.raw.trim()) chain = chain.then(function () { return renderInto(t.el, t.raw); });
      else t.el.remove();
    });
    return chain;
  };

  function send(text) {
    text = (text || '').trim();
    if (!text || state.busy || !state.session) return;
    bubble('user', null, text);
    var turn = new Turn(el('neo-agent-log'));
    var waiting = turn.append(document.createElement('div'));
    waiting.className = 'neo-agent-msg neo-agent-msg--note';
    waiting.innerHTML = '<span class="loading loading-dots loading-xs"></span>';
    var finalText = '';
    var first = state.first;
    state.first = false;
    setBusy(true);

    function handle(ev) {
      var d = ev.data || {};
      if (waiting && ev.event !== 'run.started' && ev.event !== 'message.started') {
        waiting.remove();
        waiting = null;
      }
      switch (ev.event) {
        case 'run.started':
          if (d.run_id) state.run = d.run_id;
          break;
        case 'assistant.delta':
          if (d.delta) turn.text(d.delta);
          break;
        case 'assistant.commentary':
          if (!d.already_streamed && d.text) {
            turn.text(d.text);
            turn.seg = null; // the next text starts a new paragraph
          }
          break;
        case 'tool.started':
          turn.toolStarted(d.tool_name);
          break;
        case 'tool.completed':
          turn.toolDone(d.tool_name, false);
          break;
        case 'tool.failed':
          turn.toolDone(d.tool_name, true);
          break;
        case 'approval.request':
          turn.seg = null;
          approvalCard(d);
          break;
        case 'assistant.completed':
          finalText = d.content || '';
          break;
        case 'error':
          note(d.message || 'Something went wrong.', 'neo-agent-error');
          turn.seg = null;
          break;
        default:
          break;
      }
    }

    post('/agent/chat', { session_id: state.session, message: text, context: state.context, first: first })
      .then(function (resp) {
        if (!resp.ok || !resp.body) throw new Error('The assistant did not answer (' + resp.status + ').');
        var reader = resp.body.getReader();
        var dec = new TextDecoder();
        var buf = '';
        function pump() {
          return reader.read().then(function (r) {
            if (r.done) return;
            buf += dec.decode(r.value, { stream: true });
            var parsed = parseSse(buf);
            buf = parsed.rest;
            parsed.events.forEach(handle);
            return pump();
          });
        }
        return pump();
      })
      .catch(function (e) {
        note(e.message || String(e), 'neo-agent-error');
      })
      .then(function () {
        if (waiting) waiting.remove();
        state.run = null;
        setBusy(false);
        return turn.finish(finalText);
      });
  }

  function open(context, opts) {
    opts = opts || {};
    var drawer = el('neo-agent');
    if (!drawer) return;
    drawer.classList.remove('hidden');
    document.body.classList.add('neo-agent-open');
    var fresh = !!opts.fresh;
    if (state.session && state.context === context && !fresh) {
      focusInput();
      return;
    }
    state.context = context || 'general';
    state.starter = opts.starter || null;
    state.hint = opts.hint || null;
    state.session = null;
    el('neo-agent-title').textContent = opts.title || 'Assistant';
    var log = el('neo-agent-log');
    log.innerHTML = '';
    var loading = note('Connecting to the assistant…');
    post('/agent/open', { context: state.context, fresh: fresh })
      .then(function (r) {
        return r.json().then(function (j) {
          if (!r.ok) throw new Error(j.error || 'The assistant is not reachable.');
          return j;
        });
      })
      .then(function (j) {
        loading.remove();
        state.session = j.session_id;
        var msgs = j.messages || [];
        state.first = msgs.length === 0;
        var chain = Promise.resolve();
        msgs.forEach(function (m) {
          if (m.role === 'user') bubble('user', null, m.content);
          else {
            var b = bubble('assistant', null, m.content);
            chain = chain.then(function () {
              return renderInto(b, m.content);
            });
          }
        });
        if (msgs.length) note('Continuing your earlier chat. Use New chat to start over.');
        if (!msgs.length && opts.starter) send(opts.starter);
        else if (!msgs.length) note(opts.hint || 'Ask anything about your homeserver.');
        focusInput();
      })
      .catch(function (e) {
        loading.textContent = e.message || String(e);
        loading.classList.add('neo-agent-error');
      });
  }

  function close() {
    var drawer = el('neo-agent');
    if (drawer) drawer.classList.add('hidden');
    document.body.classList.remove('neo-agent-open');
  }

  function focusInput() {
    var input = el('neo-agent-input');
    if (input) setTimeout(function () { input.focus(); }, 50);
  }

  function stop() {
    if (state.run) post('/agent/stop', { run_id: state.run });
  }

  document.addEventListener('DOMContentLoaded', function () {
    var form = el('neo-agent-form');
    if (!form) return;
    var input = el('neo-agent-input');
    form.addEventListener('submit', function (e) {
      e.preventDefault();
      var t = input.value;
      input.value = '';
      send(t);
    });
    input.addEventListener('keydown', function (e) {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        form.requestSubmit();
      }
    });
    // Buttons anywhere (also in swapped content): data-neo-agent="<context>".
    document.body.addEventListener('click', function (e) {
      var b = e.target.closest && e.target.closest('[data-neo-agent]');
      if (!b) return;
      e.preventDefault();
      open(b.getAttribute('data-neo-agent'), {
        title: b.getAttribute('data-neo-agent-title') || undefined,
        starter: b.getAttribute('data-neo-agent-starter') || undefined,
        hint: b.getAttribute('data-neo-agent-hint') || undefined,
      });
    });
  });

  // A saved proposal changes settings.toml under an open settings form: reload
  // that pane so a later form Save does not write the old values back.
  document.addEventListener('htmx:afterSwap', function () {
    var pane = document.getElementById('options-pane');
    if (!pane || pane.getAttribute('data-is-core') === 'true') return;
    var svc = pane.getAttribute('data-service');
    var cards = document.querySelectorAll(
      '#neo-agent [data-proposal][data-status="saved"]:not([data-reloaded]), #neo-agent [data-proposal][data-status="applied"]:not([data-reloaded])'
    );
    var hit = false;
    cards.forEach(function (c) {
      c.setAttribute('data-reloaded', '1');
      if ((c.getAttribute('data-services') || '').split(/\s+/).indexOf(svc) >= 0) hit = true;
    });
    if (!hit) return;
    if (window.neoConfirmLeaveConfigForm && !window.neoConfirmLeaveConfigForm()) return;
    if (typeof htmx !== 'undefined') {
      htmx.ajax('GET', '/configuration/option/' + encodeURIComponent(svc), { target: '#config-content', swap: 'innerHTML' });
    }
  });

  root.NeoAgent = {
    open: open,
    close: close,
    stop: stop,
    newChat: function () {
      if (state.busy) return;
      open(state.context, {
        title: el('neo-agent-title').textContent,
        fresh: true,
        starter: state.starter || undefined,
        hint: state.hint || undefined,
      });
    },
  };
})(typeof window !== 'undefined' ? window : globalThis);
