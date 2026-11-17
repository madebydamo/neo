// agent_chat_log.js — shared state and the message log of the assistant chat
// drawer (agent_chat.js): bubbles, notes, `@` chips, server-rendered replies,
// the drawer view (chat / past chats), title and input focus.
//
// The drawer's scripts share one internal namespace, NeoAgentChat (`A`): this
// file creates it with the drawer state; agent_chat_turn.js,
// agent_chat_approvals.js, agent_chat_status.js and agent_chat_history.js add
// their functions to it; agent_chat.js wires the drawer and exposes NeoAgent.
// Functions of a later file are called through `A.` at call time.
// Load order: agent_chat_core.js, agent_chat_refs.js, this file, the others,
// agent_chat.js last.
(function (root) {
  'use strict';

  var core = root.NeoAgentCore;
  var h = root.NeoUtil.h;
  var SPINNER = '<span class="loading loading-spinner loading-xs" aria-hidden="true"></span>';
  var A = (root.NeoAgentChat = root.NeoAgentChat || {});

  // One chat is shown at a time and at most one turn runs (in that chat).
  // phase: idle | connecting | working | waiting (approval) | stopping.
  // mode: 'stream' while this page reads the turn's events, 'poll' after a
  // reload or a broken stream (GET /agent/run/<id> until it ends).
  var state = {
    context: 'general',
    session: null,
    title: 'Assistant',
    starter: null,
    hint: null,
    first: false, // the next message creates the Hermes session
    empty: true, // the shown chat has no messages yet
    loaded: false, // the log shows `session`
    run: null,
    phase: 'idle',
    mode: null,
    started: 0,
    activity: '',
    unread: false,
    isOpen: false,
    view: 'chat',
    approvals: [],
    lastRun: null,
    poll: null,
    ticker: null,
    liveRow: null,
    seq: 0, // bumps on every chat switch; stale async results check it
  };

  // Blocked storage: the indicator just does not survive a reload.
  var session = root.NeoUtil.store('session');

  function storeGet(key) {
    return session.get(key);
  }

  function storeSet(key, value) {
    if (value == null) session.remove(key);
    else session.setJson(key, value);
  }

  function busy() {
    return state.phase !== 'idle';
  }

  function el(id) {
    return document.getElementById(id);
  }

  function scrollDown() {
    var log = el('neo-agent-log');
    if (log) log.scrollTop = log.scrollHeight;
  }

  function bubble(role, html, text) {
    var log = el('neo-agent-log');
    var b = h('div', { class: 'neo-agent-msg neo-agent-msg--' + role });
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

  /** A note with a spinner ("Loading the chat…"). */
  function spinnerNote(text) {
    var n = bubble('note', SPINNER + ' ');
    n.classList.add('neo-agent-loading');
    n.appendChild(document.createTextNode(text));
    return n;
  }

  // ---- @ references (DOM) -------------------------------------------------

  var refs = { promise: null, at: 0, byToken: {} };
  var REFS_TTL = 5 * 60 * 1000;

  /** Candidates from /agent/refs (cached server-side; refetched after 5 min). */
  function loadRefs() {
    if (refs.promise && Date.now() - refs.at < REFS_TTL) return refs.promise;
    refs.at = Date.now();
    refs.promise = fetch('/agent/refs', { credentials: 'same-origin' })
      .then(function (r) {
        return root.NeoUtil.readJson(r, 'references unavailable');
      })
      .then(function (j) {
        var items = j.items || [];
        refs.byToken = {};
        items.forEach(function (it) {
          refs.byToken[it.token] = it;
        });
        return items;
      });
    refs.promise.catch(function () {
      refs.promise = null;
    });
    return refs.promise;
  }

  function navLink(a, url) {
    a.setAttribute('href', url);
    a.setAttribute('hx-get', url);
    a.setAttribute('hx-target', '#config-content');
    a.setAttribute('hx-swap', 'innerHTML');
    a.setAttribute('hx-push-url', 'true');
  }

  /** Known `@tokens` in a user bubble become chips linking to their page. */
  function chipify(node, text) {
    var parts = core.splitRefs(text);
    if (!parts.some(function (p) { return p.ref && refs.byToken[p.ref]; })) return;
    node.textContent = '';
    parts.forEach(function (p) {
      var it = p.ref && refs.byToken[p.ref];
      if (!it) {
        node.appendChild(document.createTextNode(p.text != null ? p.text : '@' + p.ref));
        return;
      }
      var a = h('a', { class: 'neo-agent-ref', title: (core.KIND_LABEL[it.kind] || '') + ': ' + it.label },
        it.icon && h('img', { src: it.icon, alt: '' }),
        document.createTextNode('@' + it.token));
      navLink(a, it.url);
      node.appendChild(a);
    });
    if (typeof htmx !== 'undefined') htmx.process(node);
  }

  function userBubble(text) {
    var b = bubble('user', null, text);
    if (text && text.indexOf('@') >= 0) {
      loadRefs().then(function () { chipify(b, text); }, function () {});
    }
    return b;
  }

  // ---- server calls ---------------------------------------------------------

  function post(url, body) {
    return fetch(url, {
      method: 'POST',
      credentials: 'same-origin',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
  }

  function jsonOrThrow(r) {
    return root.NeoUtil.readJson(r, 'The assistant is not reachable');
  }

  /** A reply rendered server-side; its proposal links become proposal cards. */
  function renderInto(node, text) {
    return post('/agent/render', { text: text })
      .then(function (r) {
        return r.ok ? r.text() : null;
      })
      .then(function (html) {
        if (html != null) node.innerHTML = html;
        else node.textContent = text;
        if (typeof htmx !== 'undefined') htmx.process(node);
        core.proposalIds(text).forEach(function (id) {
          var slot = node.appendChild(h('div', { class: 'mt-2' }));
          if (typeof htmx !== 'undefined') {
            // The card arrives after the reply: scroll again once it is in.
            var loaded = htmx.ajax('GET', '/proposals/' + id + '/card', { target: slot, swap: 'innerHTML' });
            if (loaded && typeof loaded.then === 'function') loaded.then(scrollDown, scrollDown);
          }
        });
        scrollDown();
      });
  }

  // ---- drawer view ------------------------------------------------------------

  function setTitle(t) {
    state.title = t || 'Assistant';
    var head = el('neo-agent-title');
    if (head) head.textContent = state.title;
  }

  function clearLog() {
    A.endApprovals();
    state.liveRow = null;
    var log = el('neo-agent-log');
    if (log) log.innerHTML = '';
  }

  function showView(v) {
    state.view = v;
    var hist = el('neo-agent-history');
    var log = el('neo-agent-log');
    var form = el('neo-agent-form');
    var btn = el('neo-agent-chats');
    if (hist) hist.classList.toggle('hidden', v !== 'history');
    if (log) log.classList.toggle('hidden', v === 'history');
    if (form) form.classList.toggle('hidden', v === 'history');
    if (btn) {
      btn.setAttribute('aria-pressed', v === 'history' ? 'true' : 'false');
      btn.classList.toggle('btn-active', v === 'history');
    }
  }

  function focusInput() {
    var input = el('neo-agent-input');
    if (input && state.view === 'chat') setTimeout(function () { input.focus(); }, 50);
  }

  Object.assign(A, {
    SPINNER: SPINNER,
    state: state,
    storeGet: storeGet,
    storeSet: storeSet,
    busy: busy,
    el: el,
    scrollDown: scrollDown,
    bubble: bubble,
    note: note,
    spinnerNote: spinnerNote,
    loadRefs: loadRefs,
    userBubble: userBubble,
    post: post,
    jsonOrThrow: jsonOrThrow,
    renderInto: renderInto,
    setTitle: setTitle,
    clearLog: clearLog,
    showView: showView,
    focusInput: focusInput,
  });
})(typeof window !== 'undefined' ? window : globalThis);
