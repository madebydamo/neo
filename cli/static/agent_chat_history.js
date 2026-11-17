// agent_chat_history.js — the past chats list of the assistant chat drawer
// (agent_chat.js): GET /agent/sessions, grouped rows, reopen, delete with an
// in-row confirmation. Adds backToChat / toggleHistory to the drawer namespace
// (agent_chat_log.js).
(function (root) {
  'use strict';

  var core = root.NeoAgentCore;
  var h = root.NeoUtil.h;
  var A = root.NeoAgentChat;
  var state = A.state;
  var busy = A.busy;
  var el = A.el;

  var TRASH_SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4" aria-hidden="true"><path fill-rule="evenodd" d="M8.75 1A2.75 2.75 0 006 3.75v.443c-.795.077-1.584.176-2.365.298a.75.75 0 10.23 1.482l.149-.022.841 10.518A2.75 2.75 0 007.596 19h4.807a2.75 2.75 0 002.742-2.53l.841-10.52.149.023a.75.75 0 00.23-1.482A41.03 41.03 0 0014 4.193V3.75A2.75 2.75 0 0011.25 1h-2.5zM10 4c.84 0 1.673.025 2.5.075V3.75c0-.69-.56-1.25-1.25-1.25h-2.5c-.69 0-1.25.56-1.25 1.25v.325C8.327 4.025 9.16 4 10 4zM8.58 7.72a.75.75 0 00-1.5.06l.3 7.5a.75.75 0 101.5-.06l-.3-7.5zm4.34.06a.75.75 0 10-1.5-.06l-.3 7.5a.75.75 0 101.5.06l.3-7.5z" clip-rule="evenodd"/></svg>';

  function showHistory() {
    A.showView('history');
    var box = el('neo-agent-history');
    if (!box) return;
    box.textContent = '';
    var loading = box.appendChild(h('div', {
      class: 'neo-agent-msg neo-agent-msg--note neo-agent-loading',
      innerHTML: A.SPINNER + ' Loading your chats…',
    }));
    fetch('/agent/sessions?context=' + encodeURIComponent(state.context), { credentials: 'same-origin' })
      .then(A.jsonOrThrow)
      .then(function (j) {
        if (state.view === 'history') renderHistory(j.items || []);
      })
      .catch(function (e) {
        loading.textContent = e.message || String(e);
        loading.classList.add('neo-agent-error');
      });
  }

  function renderHistory(items) {
    var box = el('neo-agent-history');
    box.textContent = '';
    box.appendChild(h('div', { class: 'flex items-center gap-2 mb-2' },
      h('button', { type: 'button', class: 'btn btn-ghost btn-xs', text: '← Back to the chat', on: { click: backToChat } })));
    var groups = core.groupSessions(items, state.context);
    if (!groups.length) {
      box.appendChild(h('p', {
        class: 'neo-agent-msg neo-agent-msg--note',
        text: 'No earlier chats yet. A chat shows up here after its first message.',
      }));
      return;
    }
    groups.forEach(function (g) {
      box.appendChild(h('h3', { class: 'neo-agent-hist-head', text: g.label }));
      box.appendChild(h('ul', { class: 'neo-agent-hist-list' },
        ...g.items.map(function (s) { return historyRow(s, items); })));
    });
  }

  function historyRow(s, all) {
    var current = s.id === state.session;
    var meta = core.historyMeta(s);
    var locked = busy() && !current;
    var openBtn = h(
      'button',
      {
        type: 'button',
        class: 'neo-agent-hist-open',
        disabled: locked,
        title: locked ? 'Wait for the current reply or stop it first' : '',
        on: {
          click: function () {
            if (current && state.loaded) backToChat();
            else A.loadChat(s.id, { title: s.title || s.label });
          },
        },
      },
      h('span', Object.assign({ class: 'neo-agent-ref-ico', attrs: { 'aria-hidden': 'true' } }, s.icon ? {} : { text: meta.icon }),
        s.icon && h('img', { src: s.icon, alt: '' })),
      h('span', { class: 'min-w-0 flex-1' },
        h('span', { class: 'block text-sm font-medium truncate', text: s.title || s.preview || 'Chat' }),
        h('span', { class: 'block text-xs text-base-content/60 truncate', text: meta.meta })),
      current && h('span', {
        class: 'badge badge-xs shrink-0 ' + (busy() ? 'badge-primary' : 'badge-ghost'),
        text: busy() ? 'Working…' : 'Open',
      })
    );
    var del = h('button', {
      type: 'button',
      class: 'btn btn-ghost btn-xs btn-square shrink-0',
      title: 'Delete this chat',
      innerHTML: TRASH_SVG,
      disabled: current && busy(),
      attrs: { 'aria-label': 'Delete this chat' },
      on: {
        click: function () {
          confirmDelete(li, s, all);
        },
      },
    });
    var li = h('li', { class: 'neo-agent-hist-row' + (current ? ' is-current' : '') }, openBtn, del);
    return li;
  }

  /** Delete with the question in the row itself (no browser dialog). */
  function confirmDelete(li, s, all) {
    var kept = Array.prototype.slice.call(li.childNodes);
    li.textContent = '';
    li.classList.add('is-confirm');
    var q = h('span', { class: 'text-sm flex-1 min-w-0', text: 'Delete “' + (s.title || 'this chat') + '”? This cannot be undone.' });
    var yes = h('button', { type: 'button', class: 'btn btn-error btn-xs', text: 'Delete' });
    var no = h('button', { type: 'button', class: 'btn btn-ghost btn-xs', text: 'Cancel' });
    no.addEventListener('click', function () {
      li.textContent = '';
      li.classList.remove('is-confirm');
      kept.forEach(function (n) { li.appendChild(n); });
    });
    yes.addEventListener('click', function () {
      yes.disabled = true;
      no.disabled = true;
      yes.innerHTML = A.SPINNER + ' Deleting…';
      fetch('/agent/sessions/' + encodeURIComponent(s.id), { method: 'DELETE', credentials: 'same-origin' })
        .then(A.jsonOrThrow)
        .then(function () {
          if (s.id === state.session && !busy()) {
            state.session = null;
            state.loaded = false;
            state.unread = false;
            A.clearLog();
            A.persist();
            A.paintHeader();
          }
          var rest = all.filter(function (x) { return x.id !== s.id; });
          all.length = 0;
          Array.prototype.push.apply(all, rest);
          renderHistory(all);
        })
        .catch(function (e) {
          q.textContent = e.message || String(e);
          q.classList.add('text-error');
          yes.disabled = false;
          no.disabled = false;
          yes.textContent = 'Try again';
        });
    });
    li.appendChild(q);
    li.appendChild(yes);
    li.appendChild(no);
    yes.focus();
  }

  function backToChat() {
    A.showView('chat');
    if (!state.session && !busy()) A.startNew(state.context, { title: state.defaultTitle, hint: state.hint || undefined });
    else if (state.session && !state.loaded) A.loadChat(state.session, { title: state.title });
    A.focusInput();
  }

  function toggleHistory() {
    if (state.view === 'history') backToChat();
    else showHistory();
  }

  Object.assign(A, {
    backToChat: backToChat,
    toggleHistory: toggleHistory,
  });
})(typeof window !== 'undefined' ? window : globalThis);
