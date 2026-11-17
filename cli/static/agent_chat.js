// agent_chat.js — the assistant chat drawer (Hermes via /agent/*).
//
// NeoAgent.open(context, { title, starter }) opens the drawer for a context
// ("general", "service:<name>", "guide:<id>"), resumes that chat and, for a
// new chat with a starter, sends it. Turns stream Hermes' session SSE events
// (assistant.delta, tool.*, approval.request, assistant.completed, …).
// Assistant replies are rendered server-side (/agent/render, same Markdown and
// neo links as guides); /proposals/<id> links in a reply become the proposal
// card (Save / Save & apply / Reject).
//
// This file sends turns, opens chats and wires the drawer. The rest lives in
// the drawer namespace NeoAgentChat, loaded before it: agent_chat_log.js
// (state, message log, view), agent_chat_turn.js (a turn's timeline),
// agent_chat_approvals.js (approval cards), agent_chat_status.js (phase,
// status line, header, reload, polling), agent_chat_history.js (past chats).
(function (root) {
  'use strict';

  var core = root.NeoAgentCore;
  var h = root.NeoUtil.h;
  var A = root.NeoAgentChat;
  var state = A.state;
  var parseSavedRun = core.parseSavedRun;
  var parseSse = core.parseSse;
  var toolLabel = core.toolLabel;
  var toolCallsFromMessages = core.toolCallsFromMessages;

  var busy = A.busy;
  var el = A.el;
  var bubble = A.bubble;
  var note = A.note;
  var spinnerNote = A.spinnerNote;
  var loadRefs = A.loadRefs;
  var userBubble = A.userBubble;
  var post = A.post;
  var jsonOrThrow = A.jsonOrThrow;
  var renderInto = A.renderInto;
  var setTitle = A.setTitle;
  var clearLog = A.clearLog;
  var showView = A.showView;
  var focusInput = A.focusInput;
  var Turn = A.Turn;
  var findApproval = A.findApproval;
  var approvalCard = A.approvalCard;
  var endApprovals = A.endApprovals;
  var paintControls = A.paintControls;
  var paintHeader = A.paintHeader;
  var persist = A.persist;
  var setPhase = A.setPhase;
  var setActivity = A.setActivity;
  var schedulePoll = A.schedulePoll;
  var stopPoll = A.stopPoll;
  var showLiveRow = A.showLiveRow;
  var outcomeNote = A.outcomeNote;
  var markUnread = A.markUnread;
  var toggleHistory = A.toggleHistory;

  function send(text) {
    text = (text || '').trim();
    if (!text || busy() || !state.session) return;
    if (state.view !== 'chat') showView('chat');
    userBubble(text);
    state.empty = false;
    var turn = new Turn(el('neo-agent-log'));
    var waiting = turn.append(h('div', {
      class: 'neo-agent-msg neo-agent-msg--note',
      innerHTML: '<span class="loading loading-dots loading-xs"></span>',
    }));
    var finalText = '';
    var outcome = null;
    var failure = null;
    var first = state.first;
    state.first = false;
    state.mode = 'stream';
    state.run = null;
    state.started = Date.now();
    setPhase('connecting', '');

    function handle(ev) {
      var d = ev.data || {};
      if (waiting && ev.event !== 'run.started' && ev.event !== 'message.started') {
        waiting.remove();
        waiting = null;
      }
      switch (ev.event) {
        case 'run.started':
          if (d.run_id) state.run = d.run_id;
          if (state.phase === 'connecting') setPhase('working', 'Thinking');
          else {
            persist();
            paintControls();
          }
          break;
        case 'assistant.delta':
          if (d.delta) turn.text(d.delta);
          setActivity('Writing the reply');
          break;
        case 'assistant.commentary':
          if (!d.already_streamed && d.text) {
            turn.text(d.text);
            turn.seg = null; // the next text starts a new paragraph
          }
          break;
        case 'tool.started':
          turn.toolStarted(d);
          setActivity(toolLabel(d.tool_name));
          break;
        case 'tool.completed':
        case 'tool.failed':
          turn.toolDone(d, ev.event === 'tool.failed');
          var open = turn.running();
          setActivity(open.length ? toolLabel(open[open.length - 1].name) : 'Thinking');
          break;
        case 'run.completed':
        case 'run.failed':
        case 'run.cancelled':
        case 'run.interrupted':
          // The turn transcript carries each tool call's output.
          if (Array.isArray(d.messages)) turn.results(toolCallsFromMessages(d.messages));
          outcome = ev.event.slice(4);
          if (d.error) failure = typeof d.error === 'string' ? d.error : d.error.message || null;
          break;
        case 'approval.request':
          turn.seg = null;
          d.run_id = d.run_id || state.run;
          approvalCard(d);
          break;
        case 'approval.settled':
          var a = findApproval(d.request_id || d.prompt_id);
          if (a && !a.sending) a.end('This request already ended (it timed out or was answered).', 'text-base-content/60');
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
        if (!outcome && state.run) {
          // The stream ended before the turn did (connection lost, neo-web
          // restarted): the turn goes on in Hermes, so follow it by polling.
          state.mode = 'poll';
          showLiveRow();
          setPhase(state.phase === 'stopping' ? 'stopping' : 'working');
          schedulePoll(0);
          return;
        }
        // Refused before the turn began: the chat may not exist yet in Hermes.
        if (!state.run && !outcome && first) state.first = true;
        state.run = null;
        state.mode = null;
        stopPoll();
        endApprovals('The reply ended before this was answered.');
        setPhase('idle');
        return turn.finish(finalText).then(function () {
          if (outcome) outcomeNote(outcome, failure);
          markUnread();
        });
      });
  }

  // ---- chats: new, reopen ------------------------------------------------------

  /** Earlier messages of a chat (from /agent/open) into the log. */
  function replay(msgs) {
    var log = el('neo-agent-log');
    var chain = Promise.resolve();
    msgs.forEach(function (m) {
      if (m.role === 'user') userBubble(m.content);
      else if (m.role === 'tools') new Turn(log).replayTools(m.calls || []);
      else {
        var b = bubble('assistant', null, m.content);
        chain = chain.then(function () {
          return renderInto(b, m.content);
        });
      }
    });
    return chain;
  }

  /** A new chat for `context`: only an id until the first message is sent. */
  function startNew(context, opts) {
    opts = opts || {};
    var seq = ++state.seq;
    state.context = context || 'general';
    state.starter = opts.starter || null;
    state.hint = opts.hint || null;
    state.defaultTitle = opts.title || 'Assistant';
    state.session = null;
    state.loaded = true;
    state.first = true;
    state.empty = true;
    setTitle(state.defaultTitle);
    showView('chat');
    clearLog();
    var loading = spinnerNote('Connecting to the assistant…');
    loadRefs().catch(function () {});
    return post('/agent/open', { context: state.context })
      .then(jsonOrThrow)
      .then(function (j) {
        if (seq !== state.seq) return;
        loading.remove();
        state.session = j.session_id;
        if (opts.starter) send(opts.starter);
        else note(opts.hint || 'Ask anything about your homeserver.');
        focusInput();
      })
      .catch(function (e) {
        if (seq !== state.seq) return;
        loading.textContent = e.message || String(e);
        loading.classList.add('neo-agent-error');
      });
  }

  /** Reopen a chat: its latest messages, plus the live turn when one runs. */
  function loadChat(id, opts) {
    opts = opts || {};
    var seq = ++state.seq;
    state.session = id;
    // Not "loaded" until the messages are in: a poll meanwhile must not put
    // approval cards above the history (they are added below once it is).
    state.loaded = false;
    if (!opts.keepView) showView('chat');
    if (opts.title) setTitle(opts.title);
    clearLog();
    var loading = spinnerNote('Loading the chat…');
    loadRefs().catch(function () {});
    return post('/agent/open', { session_id: id })
      .then(jsonOrThrow)
      .then(function (j) {
        if (seq !== state.seq) return;
        loading.remove();
        state.loaded = true;
        if (j.context) state.context = j.context;
        var msgs = j.messages || [];
        state.first = msgs.length === 0;
        state.empty = msgs.length === 0;
        if (j.truncated) note('Earlier messages are not shown.', 'neo-agent-end');
        var rendered = replay(msgs);
        if (!msgs.length && !busy()) note('This chat has no messages yet.');
        if (busy() && state.mode === 'poll') {
          showLiveRow();
          ((state.lastRun && state.lastRun.approvals) || []).forEach(function (a) {
            a.run_id = state.run;
            approvalCard(a);
          });
        }
        persist();
        if (!opts.keepView) focusInput();
        return rendered;
      })
      .catch(function (e) {
        if (seq !== state.seq) return;
        loading.textContent = e.message || String(e);
        loading.classList.add('neo-agent-error');
      });
  }

  // ---- drawer --------------------------------------------------------------

  /**
   * Open the drawer. A running turn (or a reply that came in while the drawer
   * was closed) stays in front; otherwise every open starts a new chat for
   * `context` (reusing an empty one), and past chats are one click away.
   */
  function open(context, opts) {
    opts = opts || {};
    var drawer = el('neo-agent');
    if (!drawer) return;
    drawer.classList.remove('hidden');
    document.body.classList.add('neo-agent-open');
    state.isOpen = true;
    context = context || 'general';
    var unread = state.unread;
    state.unread = false;
    persist();
    paintHeader();
    if (busy() || (unread && state.session && !opts.starter)) {
      showView('chat');
      if (state.session && !state.loaded) loadChat(state.session, { title: state.title });
      else if (busy() && context !== state.context) {
        note('The assistant is still answering here. Stop it or wait to start another chat.');
      }
      focusInput();
      return;
    }
    if (state.session && state.loaded && state.empty && state.context === context && !opts.starter) {
      if (opts.title) setTitle(opts.title);
      showView('chat');
      focusInput();
      return;
    }
    startNew(context, opts);
  }

  function close() {
    // Only hides the drawer: a running turn keeps streaming.
    var drawer = el('neo-agent');
    if (drawer) drawer.classList.add('hidden');
    document.body.classList.remove('neo-agent-open');
    state.isOpen = false;
    paintHeader();
  }


  function stop() {
    if (!state.run || state.phase === 'stopping') return;
    var before = state.phase;
    setPhase('stopping');
    post('/agent/stop', { run_id: state.run })
      .then(function (r) {
        if (!r.ok) throw new Error('stop refused');
      })
      .catch(function () {
        if (state.phase === 'stopping') setPhase(before);
        note('Could not stop the assistant. Try again.', 'neo-agent-error');
      });
  }

  /** After a reload: follow a turn that was running, or keep the unread mark. */
  function restore() {
    var saved = parseSavedRun(A.storeGet(A.STORE_RUN));
    if (saved && saved.run_id) {
      state.session = saved.session_id;
      state.context = saved.context;
      state.run = saved.run_id;
      state.started = saved.started || Date.now();
      state.mode = 'poll';
      state.loaded = false;
      state.empty = false;
      setTitle(saved.title);
      setPhase('working');
      return;
    }
    var unread = parseSavedRun(A.storeGet(A.STORE_UNREAD));
    if (unread) {
      state.session = unread.session_id;
      state.context = unread.context;
      state.loaded = false;
      state.empty = false;
      state.unread = true;
      setTitle(unread.title);
    }
    paintHeader();
  }

  root.NeoUtil.onReady(document, function () {
    var form = el('neo-agent-form');
    if (!form) return;
    var input = el('neo-agent-input');
    var picker = root.NeoAgentRefs.picker(input, el('neo-agent-refs'), loadRefs);
    form.addEventListener('submit', function (e) {
      e.preventDefault();
      picker.close();
      if (busy()) return; // keep the draft until the reply is done
      var t = input.value;
      input.value = '';
      send(t);
    });
    input.addEventListener('keydown', function (e) {
      if (picker.key(e)) return;
      if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
        e.preventDefault();
        form.requestSubmit();
      }
    });
    // The drawer's own buttons: data-neo-agent-action="<NeoAgent method>".
    el('neo-agent').addEventListener('click', function (e) {
      var b = e.target.closest && e.target.closest('[data-neo-agent-action]');
      var fn = b && DRAWER_ACTIONS[b.getAttribute('data-neo-agent-action')];
      if (fn) fn();
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
    restore();
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
    if (hit) window.NeoUtil.reloadPane();
  });

  function newChat() {
    if (busy()) return;
    startNew(state.context, {
      title: state.defaultTitle || state.title,
      starter: state.starter || undefined,
      hint: state.hint || undefined,
    });
  }

  var DRAWER_ACTIONS = { history: toggleHistory, new: newChat, close: close, stop: stop };

  A.startNew = startNew;
  A.loadChat = loadChat;

  root.NeoAgent = {
    open: open,
    close: close,
    stop: stop,
    history: toggleHistory,
    newChat: newChat,
  };
})(typeof window !== 'undefined' ? window : globalThis);
