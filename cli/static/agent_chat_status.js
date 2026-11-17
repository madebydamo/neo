// agent_chat_status.js — where a turn of the assistant chat stands
// (agent_chat.js): phase, the status line (elapsed time, current tool), the
// drawer buttons, the header indicator, what survives a reload, and polling
// GET /agent/run/<id> for a turn this page does not stream. Adds its functions
// to the drawer namespace (agent_chat_log.js).
(function (root) {
  'use strict';

  var core = root.NeoAgentCore;
  var A = root.NeoAgentChat;
  var state = A.state;
  var busy = A.busy;
  var el = A.el;

  var STORE_RUN = 'neo-agent-run';
  var STORE_UNREAD = 'neo-agent-unread';

  // ---- status: buttons, status line, header indicator -------------------------

  function paintControls() {
    var on = busy();
    var send = el('neo-agent-send');
    var stop = el('neo-agent-stop');
    var fresh = el('neo-agent-new');
    if (send) send.classList.toggle('hidden', on);
    if (stop) {
      stop.classList.toggle('hidden', !on);
      var stopping = state.phase === 'stopping';
      stop.disabled = stopping || !state.run;
      stop.innerHTML = stopping ? A.SPINNER + ' Stopping…' : 'Stop';
    }
    if (fresh) {
      fresh.disabled = on;
      fresh.title = on ? 'Wait for the reply or stop it first' : 'Start a new chat';
    }
  }

  function paintStatus() {
    var bar = el('neo-agent-status');
    if (!bar) return;
    if (!busy()) {
      bar.classList.add('hidden');
      return;
    }
    bar.classList.remove('hidden');
    bar.setAttribute('data-phase', state.phase);
    var text = el('neo-agent-status-text');
    if (text) {
      text.textContent = core.statusLine({
        phase: state.phase,
        elapsed: state.started ? (Date.now() - state.started) / 1000 : 0,
        activity: state.activity,
        background: state.mode === 'poll',
      });
    }
  }

  /** The header Assistant button: working ring, approval color, unread dot. */
  function paintHeader() {
    var st = core.headerState(state.phase, state.unread);
    document.querySelectorAll('[data-neo-agent-launch]').forEach(function (b) {
      b.setAttribute('data-agent-state', st);
      b.title = core.HEADER_LABEL[st];
      b.setAttribute('aria-label', core.HEADER_LABEL[st]);
    });
  }

  /** Remember the running turn / the unread reply across reloads. */
  function persist() {
    var p = core.persistedState(state);
    A.storeSet(STORE_RUN, p.run);
    A.storeSet(STORE_UNREAD, p.unread);
  }

  function tick() {
    paintStatus();
    state.approvals.forEach(function (a) { a.tick(); });
  }

  function setPhase(phase, activity) {
    state.phase = phase;
    if (activity !== undefined) state.activity = activity;
    if (phase === 'idle') {
      if (state.ticker) clearInterval(state.ticker);
      state.ticker = null;
      state.activity = '';
    } else if (!state.ticker) {
      state.ticker = setInterval(tick, 1000);
    }
    paintStatus();
    paintControls();
    paintHeader();
    persist();
    // Polling also notices prompts answered or expired elsewhere.
    if (wantPoll() && !state.poll) schedulePoll(state.mode === 'poll' ? 0 : 5000);
  }

  /** The current activity ("Running a command") without repainting the rest. */
  function setActivity(text) {
    if (state.activity === text) return;
    state.activity = text;
    paintStatus();
  }

  // ---- polling a run (after a reload, a broken stream, or while waiting) -----

  function wantPoll() {
    return !!state.run && (state.mode === 'poll' || state.phase === 'waiting');
  }

  function schedulePoll(delay) {
    if (state.poll) clearTimeout(state.poll);
    state.poll = setTimeout(pollRun, delay);
  }

  function stopPoll() {
    if (state.poll) clearTimeout(state.poll);
    state.poll = null;
  }

  function pollRun() {
    state.poll = null;
    if (!wantPoll()) return;
    var run = state.run;
    fetch('/agent/run/' + encodeURIComponent(run), { credentials: 'same-origin' })
      .then(function (r) {
        if (r.status === 400 || r.status === 404) return { status: 'gone', done: true, approvals: [], approvals_known: true };
        return root.NeoUtil.readJson(r);
      })
      .then(function (j) {
        if (!j || run !== state.run) return;
        state.lastRun = j;
        var pending = (j.approvals || []).map(function (a) { return a.request_id; });
        if (j.approvals_known) {
          state.approvals.slice().forEach(function (a) {
            if (core.approvalGone(a, pending)) {
              a.end('This request already ended (it timed out or was answered).', 'text-base-content/60');
            }
          });
        }
        if (state.mode !== 'poll') return;
        if (j.done) {
          finishBackground(j);
          return;
        }
        if (state.loaded) {
          (j.approvals || []).forEach(function (a) {
            a.run_id = run;
            A.approvalCard(a);
          });
        }
        if (state.phase !== 'stopping') {
          setPhase(core.polledPhase(state.approvals.some(function (a) { return a.open; }), j.status));
        }
      })
      .catch(function () { /* try again on the next round */ })
      .then(function () {
        if (wantPoll() && !state.poll) schedulePoll(state.mode === 'poll' ? 3000 : 5000);
      });
  }

  /** "Still working…" row of a turn this page does not stream. */
  function showLiveRow() {
    if (state.liveRow && state.liveRow.isConnected) return;
    state.liveRow = A.spinnerNote('Still working on the reply. It shows up here when it is done.');
  }

  function outcomeNote(status, error) {
    var o = core.outcomeText(status, error);
    if (o) A.note(o.text, o.error ? 'neo-agent-error' : 'neo-agent-end');
  }

  function markUnread() {
    if (state.isOpen) return;
    state.unread = true;
    persist();
    paintHeader();
  }

  /** A polled turn ended: reload the chat when it is shown, then say how it ended. */
  function finishBackground(j) {
    state.run = null;
    state.mode = null;
    stopPoll();
    A.endApprovals();
    setPhase('idle');
    if (state.liveRow) state.liveRow.remove();
    state.liveRow = null;
    // Also while the chat is still loading (its messages may predate the end).
    if (state.session && (state.loaded || state.isOpen)) {
      A.loadChat(state.session, { keepView: true }).then(function () { outcomeNote(j.status, j.error); });
    }
    markUnread();
  }

  Object.assign(A, {
    STORE_RUN: STORE_RUN,
    STORE_UNREAD: STORE_UNREAD,
    paintControls: paintControls,
    paintHeader: paintHeader,
    persist: persist,
    setPhase: setPhase,
    setActivity: setActivity,
    schedulePoll: schedulePoll,
    stopPoll: stopPoll,
    showLiveRow: showLiveRow,
    outcomeNote: outcomeNote,
    markUnread: markUnread,
  });
})(typeof window !== 'undefined' ? window : globalThis);
