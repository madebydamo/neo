// agent_chat_approvals.js — approval cards of the assistant chat (agent_chat.js):
// answer buttons, the `timeout_s` countdown and how a prompt ends. Adds
// findApproval / approvalCard / endApprovals to the drawer namespace
// (agent_chat_log.js).
(function (root) {
  'use strict';

  var core = root.NeoAgentCore;
  var h = root.NeoUtil.h;
  var A = root.NeoAgentChat;
  var state = A.state;

  function findApproval(id) {
    for (var i = 0; i < state.approvals.length; i++) {
      if (id && state.approvals[i].id === id) return state.approvals[i];
    }
    return null;
  }

  /** After a prompt was answered or ended: back to "working" when none is left. */
  function approvalsChanged() {
    var open = state.approvals.some(function (a) { return a.open; });
    if (state.phase === 'waiting' && !open) A.setPhase('working', 'Continuing');
    else if (open && (state.phase === 'working' || state.phase === 'connecting')) A.setPhase('waiting');
  }

  /**
   * An approval prompt (approval.request event, or a pending prompt from
   * /agent/run). Only this card answers it: its buttons lock on the first
   * click. It counts down when Hermes gives a `timeout_s` and ends on expiry,
   * on approval.settled, when /agent/run no longer lists it, or with its turn.
   */
  function approvalCard(d) {
    if (d.request_id && findApproval(d.request_id)) return;
    var card = A.bubble('approval', '');
    var timer = h('span', { class: 'neo-agent-approval-timer' });
    card.appendChild(h('div', { class: 'flex items-baseline gap-2' },
      h('div', { class: 'font-medium text-sm flex-1', text: 'The assistant wants to do this:' }),
      timer));
    var what = d.description || d.reason;
    if (what) card.appendChild(h('div', { class: 'text-sm mt-1', text: what }));
    if (d.command) card.appendChild(h('pre', { class: 'neo-agent-cmd', text: d.command }));
    var row = h('div', { class: 'flex flex-wrap gap-2 mt-2' });
    var msg = h('div', { class: 'neo-agent-approval-msg hidden', attrs: { role: 'status' } });
    var ctl = {
      id: d.request_id || null,
      open: true,
      card: card,
      expires: core.approvalExpires(d.timeout_s, Date.now()),
    };
    function lock(on) {
      row.querySelectorAll('button').forEach(function (b) { b.disabled = on; });
    }
    function say(text, cls) {
      msg.textContent = text;
      msg.className = 'neo-agent-approval-msg' + (cls ? ' ' + cls : '');
    }
    /** The prompt is over: no more answers. */
    ctl.end = function (text, cls) {
      if (!ctl.open) return;
      ctl.open = false;
      lock(true);
      timer.textContent = '';
      card.classList.add('is-ended');
      if (text) say(text, cls);
      state.approvals = state.approvals.filter(function (a) { return a !== ctl; });
      approvalsChanged();
    };
    ctl.tick = function () {
      if (!ctl.open || !ctl.expires || ctl.sending) return;
      var left = (ctl.expires - Date.now()) / 1000;
      if (left <= 0) {
        ctl.end('This request timed out. Ask the assistant to try again if you still want it.', 'text-warning');
        return;
      }
      timer.textContent = core.countdownText(left);
      timer.classList.toggle('is-soon', left < 60);
    };
    (d.choices || ['once', 'deny']).forEach(function (c) {
      var btn = h('button', {
        type: 'button',
        class: 'btn btn-xs ' + (c === 'deny' ? 'btn-ghost' : c === 'once' ? 'btn-primary' : ''),
        text: core.APPROVAL_LABELS[c] || c,
      });
      btn.addEventListener('click', function () {
        if (!ctl.open || ctl.sending) return;
        ctl.sending = true;
        lock(true);
        btn.classList.add('btn-disabled');
        say('Sending your answer…');
        A.post('/agent/approve', core.approvalAnswer(d, state.run, c))
          .then(function (r) {
            if (r.ok) return { ok: true };
            return r.json().then(function (j) { return j || {}; }, function () { return {}; });
          }, function () {
            return { error: 'The assistant is not reachable right now. Try again in a moment.' };
          })
          .then(function (j) {
            ctl.sending = false;
            var res = core.approvalResult(j, c);
            if (res.end) {
              ctl.end(res.text, res.cls);
            } else {
              // Not taken but still open: let the user try again.
              lock(false);
              btn.classList.remove('btn-disabled');
              say(res.text, res.cls);
              ctl.tick();
            }
          });
      });
      row.appendChild(btn);
    });
    card.appendChild(row);
    card.appendChild(msg);
    state.approvals.push(ctl);
    ctl.tick();
    approvalsChanged();
    A.scrollDown();
    return ctl;
  }

  /** End every open prompt (the turn is over, or the log is replaced). */
  function endApprovals(text) {
    state.approvals.slice().forEach(function (a) { a.end(text, 'text-base-content/60'); });
    state.approvals = [];
  }

  Object.assign(A, {
    findApproval: findApproval,
    approvalCard: approvalCard,
    endApprovals: endApprovals,
  });
})(typeof window !== 'undefined' ? window : globalThis);
