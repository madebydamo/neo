// agent_chat_turn.js — one turn of the assistant chat as a timeline (text
// segments, collapsible tool calls) for agent_chat.js. Adds `Turn` to the
// drawer namespace (agent_chat_log.js).
(function (root) {
  'use strict';

  var core = root.NeoAgentCore;
  var h = root.NeoUtil.h;
  var A = root.NeoAgentChat;
  var toolLabel = core.toolLabel;
  var toolDetail = core.toolDetail;

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
    A.scrollDown();
    return el;
  };

  Turn.prototype.text = function (delta) {
    var t = this.texts[this.texts.length - 1];
    if (!t || this.seg !== t.el) {
      var el = h('div', { class: 'neo-agent-msg neo-agent-msg--assistant neo-agent-typing' });
      t = { el: el, raw: '' };
      this.texts.push(t);
      this.append(el);
    }
    t.raw += delta;
    // Segments after a tool call start with blank lines; pre-wrap would show them.
    t.el.textContent = t.raw.replace(/^\s+/, '');
    A.scrollDown();
  };

  Turn.prototype.group = function () {
    var g = this.groups[this.groups.length - 1];
    if (g && this.seg === g.el) return g;
    var sum = h('summary');
    var body = h('div', { class: 'neo-agent-tools-body' });
    var el = h('details', { class: 'neo-agent-tools' }, sum, body);
    g = { el: el, sum: sum, body: body, lines: [] };
    this.groups.push(g);
    this.append(el);
    return g;
  };

  Turn.prototype.summarize = function (g, running) {
    var open = g.lines.filter(function (l) { return l.status === 'running'; });
    if (running && open.length) {
      g.sum.innerHTML = '<span class="loading loading-dots loading-xs"></span> ';
      g.sum.appendChild(document.createTextNode(toolLabel(open[open.length - 1].name)));
    } else {
      g.sum.textContent = core.toolGroupSummary(g.lines);
    }
  };

  /** Summary row of one tool call: icon, plain label, duration. */
  function paintLine(line) {
    var s = line.sum;
    s.textContent = '';
    s.appendChild(h('span', {
      class: 'neo-agent-tool-icon neo-agent-tool-icon--' + line.status,
      text: core.TOOL_STATUS_ICON[line.status] || '✓',
    }));
    s.appendChild(document.createTextNode(toolLabel(line.name)));
    var d = toolDetail(line);
    if (d.duration) s.appendChild(h('span', { class: 'neo-agent-tool-time', text: d.duration }));
    if (line.el.open) paintDetail(line);
  }

  /** Expanded body: tool name, pretty input, output (filled lazily on open). */
  function paintDetail(line) {
    var d = toolDetail(line);
    var body = line.body;
    body.textContent = '';
    body.appendChild(h('div', { class: 'neo-agent-tool-meta', text: d.meta }));
    d.sections.forEach(function (sec) {
      body.appendChild(h('div', { class: 'neo-agent-tool-label', text: sec.label }));
      body.appendChild(h(sec.pre ? 'pre' : 'div', {
        class: sec.pre ? 'neo-agent-tool-pre' : sec.mono ? 'neo-agent-tool-name' : 'neo-agent-tool-note',
        text: sec.text,
      }));
    });
  }

  /** A collapsible tool call inside group `g`. */
  function toolLine(g, data) {
    var sum = h('summary');
    var body = h('div', { class: 'neo-agent-tool-body' });
    var el = h('details', { class: 'neo-agent-tool' }, sum, body);
    g.body.appendChild(el);
    var line = {
      name: data.name || '',
      args: data.args,
      preview: data.preview || '',
      started: data.started,
      ended: data.ended,
      result: data.result,
      status: data.status || 'running',
      final: !!data.final,
      el: el,
      sum: sum,
      body: body,
      group: g,
    };
    el.addEventListener('toggle', function () {
      if (el.open) paintDetail(line);
    });
    g.lines.push(line);
    paintLine(line);
    return line;
  }

  Turn.prototype.toolStarted = function (d) {
    var g = this.group();
    var line = toolLine(g, { name: d.tool_name, args: d.args, preview: d.preview, started: d.ts });
    this.lines.push(line);
    this.summarize(g, true);
    A.scrollDown();
  };

  Turn.prototype.toolDone = function (d, failed) {
    var line = core.firstRunning(this.lines, d.tool_name);
    if (!line) return;
    line.status = failed ? 'error' : 'done';
    line.ended = d.ts;
    paintLine(line);
    this.summarize(line.group, true);
  };

  /** Results from the turn transcript: nth call of a tool ↔ nth line of it. */
  Turn.prototype.results = function (calls) {
    core.matchToolResults(this.lines, calls);
  };

  /** Earlier tool calls of a resumed chat (history `{role: "tools"}` entry). */
  Turn.prototype.replayTools = function (calls) {
    var g = this.group();
    var self = this;
    calls.forEach(function (c) {
      self.lines.push(toolLine(g, { name: c.name, args: c.args, result: c.result, status: 'done', final: true }));
    });
    this.summarize(g, false);
  };

  /** Tool calls of this turn still running. */
  Turn.prototype.running = function () {
    return this.lines.filter(function (l) { return l.status === 'running'; });
  };

  Turn.prototype.finish = function (finalText) {
    var self = this;
    this.lines.forEach(function (l) {
      if (l.status === 'running') l.status = 'stopped';
      l.final = true;
      paintLine(l);
    });
    this.groups.forEach(function (g) { self.summarize(g, false); });
    // The final answer arrives in full with assistant.completed; it is the
    // streamed text when there was any.
    if (!this.texts.length && finalText && finalText.trim()) this.text(finalText);
    var chain = Promise.resolve();
    this.texts.forEach(function (t) {
      t.el.classList.remove('neo-agent-typing');
      if (t.raw.trim()) chain = chain.then(function () { return A.renderInto(t.el, t.raw); });
      else t.el.remove();
    });
    return chain;
  };

  A.Turn = Turn;
})(typeof window !== 'undefined' ? window : globalThis);
