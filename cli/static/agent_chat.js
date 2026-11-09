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

  // ---- @ references -------------------------------------------------------
  // Same token rules as agent_refs.rs parse_refs: `@` after the start, a space,
  // an opening bracket or quote; token chars [A-Za-z0-9_.:-]; trailing . : -
  // are punctuation, not part of the token.

  var TOKEN_CHAR = /[A-Za-z0-9_.:\-]/;
  var BEFORE_AT = /[\s([{"']/;

  function trimToken(t) {
    return t.replace(/[.:\-]+$/, '');
  }

  /** The `@query` being typed at `caret`: { start (index of @), query } or null. */
  function refQuery(text, caret) {
    var i = caret;
    while (i > 0 && TOKEN_CHAR.test(text.charAt(i - 1))) i--;
    if (i === 0 || text.charAt(i - 1) !== '@') return null;
    var at = i - 1;
    if (at > 0 && !BEFORE_AT.test(text.charAt(at - 1))) return null;
    return { start: at, query: text.slice(i, caret) };
  }

  /** Text split into [{ text } | { ref }] segments for chips in a bubble. */
  function splitRefs(text) {
    var out = [];
    var buf = '';
    var s = String(text || '');
    var i = 0;
    while (i < s.length) {
      var c = s.charAt(i);
      if (c === '@' && (i === 0 || BEFORE_AT.test(s.charAt(i - 1)))) {
        var j = i + 1;
        while (j < s.length && TOKEN_CHAR.test(s.charAt(j))) j++;
        var tok = trimToken(s.slice(i + 1, j));
        if (/^[A-Za-z0-9]/.test(tok)) {
          if (buf) out.push({ text: buf });
          buf = '';
          out.push({ ref: tok });
          i += 1 + tok.length;
          continue;
        }
      }
      buf += c;
      i++;
    }
    if (buf) out.push({ text: buf });
    return out;
  }

  function isBoundary(text, i) {
    if (i === 0) return true;
    var p = text.charAt(i - 1);
    var c = text.charAt(i);
    if (/[\s._:\-\/›]/.test(p)) return true;
    return /[a-z0-9]/.test(p) && /[A-Z]/.test(c);
  }

  /**
   * Fuzzy score of `query` against `text` (higher is better), or -1 when the
   * query is not a subsequence. Rewards matches at word starts (also camelCase
   * and after . : -), consecutive runs and a match at the very start; gaps
   * cost a little. "nxtc" matches "nextcloud".
   */
  function fuzzyScore(query, text) {
    var q = String(query || '').toLowerCase();
    var raw = String(text || '');
    var t = raw.toLowerCase();
    if (!q) return 0;
    var best = -1;
    // Try each position of the first query char as the anchor; greedy after.
    for (var start = t.indexOf(q.charAt(0)); start >= 0; start = t.indexOf(q.charAt(0), start + 1)) {
      var score = 0;
      var ti = start;
      var prev = -2;
      var ok = true;
      for (var qi = 0; qi < q.length; qi++) {
        var ch = q.charAt(qi);
        var found = -1;
        // Prefer a word-start occurrence nearby over the very next one.
        var next = t.indexOf(ch, ti);
        if (next < 0) { ok = false; break; }
        found = next;
        if (prev + 1 !== next && !isBoundary(raw, next)) {
          for (var k = next + 1; k < t.length && k < next + 24; k++) {
            if (t.charAt(k) === ch && isBoundary(raw, k)) { found = k; break; }
          }
        }
        score += 1;
        if (found === prev + 1) score += 3;
        if (isBoundary(raw, found)) score += 4;
        if (found === 0) score += 6;
        if (prev >= 0) score -= Math.min(found - prev - 1, 8) * 0.15;
        prev = found;
        ti = found + 1;
      }
      if (ok) {
        if (q === t) score += 10;
        else if (t.indexOf(q) === 0) score += 5;
        score -= t.length * 0.01;
        if (score > best) best = score;
      }
    }
    return best;
  }

  /**
   * Best `limit` candidates for `query`. Empty query: installed services and
   * guides in server order. Settings rank below apps and guides unless the
   * query names one (contains a dot); installed beats not installed and
   * plumbing (advanced) comes last.
   */
  function rankRefs(items, query, limit) {
    limit = limit || 8;
    var q = String(query || '');
    if (!q) {
      return items.filter(function (it) {
        return it.kind !== 'option' && (it.kind !== 'service' || it.enabled);
      }).slice(0, limit);
    }
    var dotted = q.indexOf('.') >= 0;
    var scored = [];
    items.forEach(function (it, idx) {
      var s1 = fuzzyScore(q, it.token);
      var s2 = fuzzyScore(q, it.label) - 1;
      var s = Math.max(s1, s2);
      if (s < 0) return;
      if (it.enabled) s += 2;
      if (it.advanced) s -= 3;
      if (it.kind === 'option') s += dotted ? 2 : -3;
      scored.push({ it: it, s: s, idx: idx });
    });
    scored.sort(function (a, b) {
      return b.s - a.s || a.idx - b.idx;
    });
    return scored.slice(0, limit).map(function (x) {
      return x.it;
    });
  }

  /** Text after inserting `@token ` for the query at `start..caret`. */
  function insertRef(text, start, caret, token) {
    var after = text.slice(caret);
    var rest = after.replace(/^[A-Za-z0-9_.:\-]*/, '');
    var gap = /^\s/.test(rest) ? '' : ' ';
    var head = text.slice(0, start) + '@' + token + gap;
    return { text: head + rest, caret: head.length };
  }

  // ---- tool call details ----------------------------------------------------

  var MAX_DETAIL = 6000;

  function clip(s, max) {
    max = max || MAX_DETAIL;
    s = String(s);
    if (s.length <= max) return s;
    return s.slice(0, max) + '\n… (' + (s.length - max) + ' more characters)';
  }

  /** Pretty JSON for arguments / results; JSON text is parsed first. */
  function prettyValue(v, max) {
    if (v == null || v === '') return '';
    if (typeof v === 'string') {
      var t = v.trim();
      if (/^[\[{]/.test(t)) {
        try {
          return clip(JSON.stringify(JSON.parse(t), null, 2), max);
        } catch (e) { /* plain text */ }
      }
      return clip(v, max);
    }
    try {
      return clip(JSON.stringify(v, null, 2), max);
    } catch (e) {
      return clip(String(v), max);
    }
  }

  function formatDuration(sec) {
    if (sec == null || !isFinite(sec) || sec < 0) return '';
    if (sec < 1) return Math.max(1, Math.round(sec * 1000)) + ' ms';
    if (sec < 60) return (Math.round(sec * 10) / 10) + ' s';
    var m = Math.floor(sec / 60);
    return m + ' min ' + Math.round(sec - m * 60) + ' s';
  }

  /** Tool calls of a turn transcript (run.completed `messages`), with results. */
  function toolCallsFromMessages(messages) {
    var calls = [];
    var byId = {};
    (messages || []).forEach(function (m) {
      if (!m) return;
      if (m.role === 'assistant' && Array.isArray(m.tool_calls)) {
        m.tool_calls.forEach(function (tc) {
          var f = tc.function || tc;
          var args = f.arguments;
          if (typeof args === 'string') {
            try { args = JSON.parse(args); } catch (e) { /* keep text */ }
          }
          var c = { id: tc.id || tc.call_id || '', name: f.name || '', args: args };
          calls.push(c);
          if (c.id) byId[c.id] = c;
        });
      } else if (m.role === 'tool') {
        var c = byId[m.tool_call_id];
        if (c && c.result === undefined) {
          var content = m.content;
          if (Array.isArray(content)) {
            content = content.map(function (p) { return (p && p.text) || ''; }).join('\n');
          }
          c.result = content == null ? '' : content;
        }
      }
    });
    return calls;
  }

  var STATUS_TEXT = { running: 'Running', done: 'Done', error: 'Failed', stopped: 'Stopped' };

  /**
   * What the expanded tool call shows: status line + sections (label, text).
   * `line`: { name, args, preview, status, started, ended, result }.
   */
  function toolDetail(line) {
    var meta = [STATUS_TEXT[line.status] || 'Done'];
    var dur = line.started != null && line.ended != null ? formatDuration(line.ended - line.started) : '';
    if (dur) meta.push(dur);
    var sections = [{ label: 'Tool', text: String(line.name || ''), mono: true }];
    var args = prettyValue(line.args, 4000);
    if (args) sections.push({ label: 'Input', text: args, pre: true });
    else if (line.preview) sections.push({ label: 'Input', text: String(line.preview), pre: true });
    if (line.result !== undefined && line.result !== null) {
      sections.push({ label: 'Output', text: prettyValue(line.result) || '(empty)', pre: true });
    } else if (line.status === 'running') {
      sections.push({ label: 'Output', text: 'Waiting for the result…' });
    } else if (!line.final) {
      sections.push({ label: 'Output', text: 'Shown when the reply is finished.' });
    } else {
      sections.push({ label: 'Output', text: 'No output was recorded.' });
    }
    return { status: line.status || 'done', meta: meta.join(' · '), duration: dur, sections: sections };
  }

  // ---- chat status, history list ---------------------------------------------

  /** Elapsed / remaining seconds as a clock: 42 → "0:42", 3723 → "1:02:03". */
  function clockText(sec) {
    sec = Math.max(0, Math.floor(sec || 0));
    var h = Math.floor(sec / 3600);
    var m = Math.floor((sec % 3600) / 60);
    var s = sec % 60;
    var ss = (s < 10 ? '0' : '') + s;
    if (h) return h + ':' + (m < 10 ? '0' : '') + m + ':' + ss;
    return m + ':' + ss;
  }

  /** "just now", "5 min ago", "3 h ago", "yesterday", "4 days ago", else a date. */
  function relativeTime(tsSec, nowMs) {
    if (!tsSec) return '';
    var now = nowMs == null ? Date.now() : nowMs;
    var diff = Math.max(0, now / 1000 - tsSec);
    if (diff < 60) return 'just now';
    if (diff < 3600) return Math.floor(diff / 60) + ' min ago';
    if (diff < 86400) return Math.floor(diff / 3600) + ' h ago';
    var days = Math.floor(diff / 86400);
    if (days === 1) return 'yesterday';
    if (days < 7) return days + ' days ago';
    var d = new Date(tsSec * 1000);
    var months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    var out = d.getDate() + ' ' + months[d.getMonth()];
    if (d.getFullYear() !== new Date(now).getFullYear()) out += ' ' + d.getFullYear();
    return out;
  }

  /** Approval card countdown: "Expires in 4:05", or "Expired". */
  function countdownText(remainingSec) {
    if (remainingSec == null || !isFinite(remainingSec)) return '';
    if (remainingSec <= 0) return 'Expired';
    return 'Expires in ' + clockText(Math.ceil(remainingSec));
  }

  /**
   * The status line under the chat while a turn runs.
   * `s`: { phase: connecting|working|waiting|stopping, elapsed (s), activity, background }.
   */
  function statusLine(s) {
    var clock = clockText(s.elapsed || 0);
    switch (s.phase) {
      case 'connecting':
        return 'Connecting to the assistant…';
      case 'waiting':
        return 'Waiting for your approval · ' + clock;
      case 'stopping':
        return 'Stopping…';
      case 'working':
        if (s.background) return 'Still working · ' + clock;
        return ['Working', clock].concat(s.activity ? [s.activity] : []).join(' · ');
      default:
        return '';
    }
  }

  /**
   * Past chats as list sections, newest first in each. From a page with its
   * own context (an app, a guide), that page's chats come first.
   */
  function groupSessions(items, context) {
    var sorted = (items || []).slice().sort(function (a, b) {
      return (b.last_active || 0) - (a.last_active || 0);
    });
    if (!context || context === 'general') {
      return sorted.length ? [{ id: 'all', label: 'Recent chats', items: sorted }] : [];
    }
    var here = sorted.filter(function (s) { return s.here; });
    var other = sorted.filter(function (s) { return !s.here; });
    var out = [];
    if (here.length) out.push({ id: 'here', label: 'About this page', items: here });
    if (other.length) out.push({ id: 'other', label: here.length ? 'Other chats' : 'Recent chats', items: other });
    return out;
  }

  /** How a finished turn is shown: { text, error } or null for a normal end. */
  function outcomeText(status, error) {
    switch (status) {
      case 'cancelled':
        return { text: 'Stopped. The assistant did not finish this reply.', error: false };
      case 'failed':
        return { text: 'The reply failed' + (error ? ': ' + error : '.'), error: true };
      case 'interrupted':
        return { text: 'The assistant was restarted before it finished. Ask again to continue.', error: true };
      default:
        return null;
    }
  }

  /** A remembered running turn (sessionStorage text) → its fields, or null. */
  function parseSavedRun(raw) {
    if (!raw) return null;
    var v;
    try {
      v = JSON.parse(raw);
    } catch (e) {
      return null;
    }
    if (!v || typeof v !== 'object') return null;
    if (!/^neo-[a-z0-9-]{1,96}$/.test(String(v.session_id || ''))) return null;
    if (v.run_id != null && !/^run_[0-9a-f]{1,60}$/.test(String(v.run_id))) return null;
    return {
      session_id: v.session_id,
      run_id: v.run_id || null,
      context: typeof v.context === 'string' ? v.context : 'general',
      title: typeof v.title === 'string' && v.title ? v.title : 'Assistant',
      started: typeof v.started === 'number' ? v.started : 0,
    };
  }

  var api = {
    clockText: clockText,
    relativeTime: relativeTime,
    countdownText: countdownText,
    statusLine: statusLine,
    groupSessions: groupSessions,
    outcomeText: outcomeText,
    parseSavedRun: parseSavedRun,
    parseSse: parseSse,
    proposalIds: proposalIds,
    toolLabel: toolLabel,
    refQuery: refQuery,
    splitRefs: splitRefs,
    fuzzyScore: fuzzyScore,
    rankRefs: rankRefs,
    insertRef: insertRef,
    prettyValue: prettyValue,
    formatDuration: formatDuration,
    toolCallsFromMessages: toolCallsFromMessages,
    toolDetail: toolDetail,
  };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  if (typeof document === 'undefined') return;

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

  var STORE_RUN = 'neo-agent-run';
  var STORE_UNREAD = 'neo-agent-unread';

  // Blocked storage: the indicator just does not survive a reload.
  var session = window.NeoUtil.store('session');

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

  /** A note with a spinner ("Loading the chat…"). */
  function spinnerNote(text) {
    var n = bubble('note', '<span class="loading loading-spinner loading-xs" aria-hidden="true"></span> ');
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
        if (!r.ok) throw new Error('references unavailable (' + r.status + ')');
        return r.json();
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

  var KIND_LABEL = { service: 'App', option: 'Setting', guide: 'Guide', privacy: 'Privacy' };

  function navLink(a, url) {
    a.setAttribute('href', url);
    a.setAttribute('hx-get', url);
    a.setAttribute('hx-target', '#config-content');
    a.setAttribute('hx-swap', 'innerHTML');
    a.setAttribute('hx-push-url', 'true');
  }

  /** Known `@tokens` in a user bubble become chips linking to their page. */
  function chipify(node, text) {
    var parts = splitRefs(text);
    if (!parts.some(function (p) { return p.ref && refs.byToken[p.ref]; })) return;
    node.textContent = '';
    parts.forEach(function (p) {
      var it = p.ref && refs.byToken[p.ref];
      if (!it) {
        node.appendChild(document.createTextNode(p.text != null ? p.text : '@' + p.ref));
        return;
      }
      var a = document.createElement('a');
      a.className = 'neo-agent-ref';
      navLink(a, it.url);
      a.title = (KIND_LABEL[it.kind] || '') + ': ' + it.label;
      if (it.icon) {
        var img = document.createElement('img');
        img.src = it.icon;
        img.alt = '';
        a.appendChild(img);
      }
      a.appendChild(document.createTextNode('@' + it.token));
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

  /**
   * The `@` popup on the chat input: opens while an `@query` is typed, filters
   * fuzzily, ↑/↓ choose, Enter/Tab insert, Esc closes, click inserts.
   * `key(e)` returns true when it handled the key.
   */
  function refPicker(input, pop) {
    var st = { open: false, q: null, list: [], sel: 0, seq: 0 };
    if (!pop) return { key: function () { return false; }, close: function () {} };

    function close() {
      st.open = false;
      st.list = [];
      pop.classList.add('hidden');
      input.setAttribute('aria-expanded', 'false');
      input.removeAttribute('aria-activedescendant');
    }

    function row(it, i) {
      var r = document.createElement('div');
      r.id = 'neo-agent-ref-' + i;
      r.setAttribute('role', 'option');
      r.setAttribute('aria-selected', i === st.sel ? 'true' : 'false');
      r.className = 'neo-agent-ref-opt' + (i === st.sel ? ' is-active' : '');
      var ico = document.createElement('span');
      ico.className = 'neo-agent-ref-ico';
      if (it.icon) {
        var img = document.createElement('img');
        img.src = it.icon;
        img.alt = '';
        ico.appendChild(img);
      } else {
        ico.textContent = it.kind === 'option' ? '⚙' : it.kind === 'guide' ? '☰' : it.kind === 'privacy' ? '◎' : '▣';
      }
      r.appendChild(ico);
      var main = document.createElement('span');
      main.className = 'min-w-0 flex-1';
      var top = document.createElement('span');
      top.className = 'flex items-baseline gap-1.5 min-w-0';
      var label = document.createElement('span');
      label.className = 'text-sm font-medium truncate';
      label.textContent = it.label;
      var tok = document.createElement('span');
      tok.className = 'text-xs font-mono text-base-content/60 truncate';
      tok.textContent = '@' + it.token;
      top.appendChild(label);
      top.appendChild(tok);
      main.appendChild(top);
      var hint = [];
      if (it.hint) hint.push(it.hint);
      if (it.kind === 'service' && !it.enabled) hint.push('not installed');
      if (it.secret) hint.push('secret, value stays hidden');
      if (hint.length) {
        var h = document.createElement('span');
        h.className = 'block text-xs text-base-content/60 truncate';
        h.textContent = hint.join(' · ');
        main.appendChild(h);
      }
      r.appendChild(main);
      var badge = document.createElement('span');
      badge.className = 'badge badge-ghost badge-xs shrink-0';
      badge.textContent = KIND_LABEL[it.kind] || it.kind;
      r.appendChild(badge);
      r.addEventListener('mousedown', function (e) {
        e.preventDefault(); // keep the focus in the input
        pick(i);
      });
      r.addEventListener('mousemove', function () {
        if (st.sel !== i) { st.sel = i; paint(); }
      });
      return r;
    }

    function paint() {
      pop.textContent = '';
      if (!st.list.length) {
        var none = document.createElement('div');
        none.className = 'px-3 py-2 text-xs text-base-content/60';
        none.textContent = st.loading ? 'Loading…' : st.error ? st.error : 'Nothing matches @' + st.q.query + '.';
        pop.appendChild(none);
      }
      st.list.forEach(function (it, i) {
        pop.appendChild(row(it, i));
      });
      var foot = document.createElement('div');
      foot.className = 'neo-agent-ref-foot';
      foot.textContent = '↑↓ choose · Enter insert · Esc close';
      pop.appendChild(foot);
      pop.classList.remove('hidden');
      input.setAttribute('aria-expanded', 'true');
      if (st.list.length) input.setAttribute('aria-activedescendant', 'neo-agent-ref-' + st.sel);
      var act = pop.querySelector('.is-active');
      if (act && act.scrollIntoView) act.scrollIntoView({ block: 'nearest' });
    }

    function update() {
      var q = refQuery(input.value, input.selectionStart || 0);
      if (!q || input.selectionStart !== input.selectionEnd) return close();
      var changed = !st.open || !st.q || st.q.query !== q.query || st.q.start !== q.start;
      st.q = q;
      st.open = true;
      if (!changed) return;
      st.sel = 0;
      var seq = ++st.seq;
      st.loading = true;
      st.error = null;
      if (!st.list.length) paint();
      loadRefs().then(
        function (items) {
          if (seq !== st.seq || !st.open) return;
          st.loading = false;
          st.list = rankRefs(items, q.query, 8);
          paint();
        },
        function () {
          if (seq !== st.seq || !st.open) return;
          st.loading = false;
          st.list = [];
          st.error = 'References are not available right now.';
          paint();
        }
      );
    }

    function pick(i) {
      var it = st.list[i];
      if (!it || !st.q) return close();
      var caret = input.selectionStart || 0;
      var out = insertRef(input.value, st.q.start, caret, it.token);
      input.value = out.text;
      input.setSelectionRange(out.caret, out.caret);
      close();
      input.focus();
    }

    function key(e) {
      if (!st.open || e.isComposing) return false;
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        close();
        return true;
      }
      if (!st.list.length) return false;
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        var n = st.list.length;
        st.sel = (st.sel + (e.key === 'ArrowDown' ? 1 : n - 1)) % n;
        paint();
        return true;
      }
      if ((e.key === 'Enter' && !e.shiftKey) || e.key === 'Tab') {
        e.preventDefault();
        pick(st.sel);
        return true;
      }
      return false;
    }

    input.setAttribute('aria-autocomplete', 'list');
    input.setAttribute('aria-controls', pop.id);
    input.setAttribute('aria-expanded', 'false');
    input.addEventListener('input', update);
    input.addEventListener('click', update);
    input.addEventListener('keyup', function (e) {
      if (/^(ArrowLeft|ArrowRight|Home|End)$/.test(e.key)) update();
    });
    input.addEventListener('blur', function () {
      setTimeout(close, 120);
    });
    return { key: key, close: close };
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
            // The card arrives after the reply: scroll again once it is in.
            var loaded = htmx.ajax('GET', '/proposals/' + id + '/card', { target: slot, swap: 'innerHTML' });
            if (loaded && typeof loaded.then === 'function') loaded.then(scrollDown, scrollDown);
          }
        });
        scrollDown();
      });
  }

  // ---- approvals ------------------------------------------------------------

  function findApproval(id) {
    for (var i = 0; i < state.approvals.length; i++) {
      if (id && state.approvals[i].id === id) return state.approvals[i];
    }
    return null;
  }

  /** After a prompt was answered or ended: back to "working" when none is left. */
  function approvalsChanged() {
    var open = state.approvals.some(function (a) { return a.open; });
    if (state.phase === 'waiting' && !open) setPhase('working', 'Continuing');
    else if (open && (state.phase === 'working' || state.phase === 'connecting')) setPhase('waiting');
  }

  /**
   * An approval prompt (approval.request event, or a pending prompt from
   * /agent/run). Only this card answers it: its buttons lock on the first
   * click. It counts down when Hermes gives a `timeout_s` and ends on expiry,
   * on approval.settled, when /agent/run no longer lists it, or with its turn.
   */
  function approvalCard(d) {
    if (d.request_id && findApproval(d.request_id)) return;
    var card = bubble('approval', '');
    var head = document.createElement('div');
    head.className = 'flex items-baseline gap-2';
    var title = document.createElement('div');
    title.className = 'font-medium text-sm flex-1';
    title.textContent = 'The assistant wants to do this:';
    var timer = document.createElement('span');
    timer.className = 'neo-agent-approval-timer';
    head.appendChild(title);
    head.appendChild(timer);
    card.appendChild(head);
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
    var msg = document.createElement('div');
    msg.className = 'neo-agent-approval-msg hidden';
    msg.setAttribute('role', 'status');
    var ctl = {
      id: d.request_id || null,
      open: true,
      card: card,
      expires: typeof d.timeout_s === 'number' && d.timeout_s > 0 ? Date.now() + d.timeout_s * 1000 : null,
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
      timer.textContent = countdownText(left);
      timer.classList.toggle('is-soon', left < 60);
    };
    var labels = { once: 'Allow once', session: 'Allow for this chat', always: 'Always allow', deny: 'Deny' };
    (d.choices || ['once', 'deny']).forEach(function (c) {
      var btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'btn btn-xs ' + (c === 'deny' ? 'btn-ghost' : c === 'once' ? 'btn-primary' : '');
      btn.textContent = labels[c] || c;
      btn.addEventListener('click', function () {
        if (!ctl.open || ctl.sending) return;
        ctl.sending = true;
        lock(true);
        btn.classList.add('btn-disabled');
        say('Sending your answer…');
        // Hermes' session runs answer one exact prompt: echo its id and generation back.
        var answer = { run_id: d.run_id || state.run, choice: c };
        if (d.request_id) answer.request_id = d.request_id;
        if (typeof d.execution_generation === 'number') answer.execution_generation = d.execution_generation;
        post('/agent/approve', answer)
          .then(function (r) {
            if (r.ok) return { ok: true };
            return r.json().then(function (j) { return j || {}; }, function () { return {}; });
          }, function () {
            return { error: 'The assistant is not reachable right now. Try again in a moment.' };
          })
          .then(function (j) {
            ctl.sending = false;
            if (j.ok) {
              ctl.end(c === 'deny' ? 'Denied.' : 'Allowed. The assistant continues.', c === 'deny' ? '' : 'text-success');
            } else if (j.ended) {
              ctl.end(j.error, 'text-warning');
            } else {
              // Not taken but still open: let the user try again.
              lock(false);
              btn.classList.remove('btn-disabled');
              say(j.error || 'Could not send the answer.', 'text-error');
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
    scrollDown();
    return ctl;
  }

  /** End every open prompt (the turn is over, or the log is replaced). */
  function endApprovals(text) {
    state.approvals.slice().forEach(function (a) { a.end(text, 'text-base-content/60'); });
    state.approvals = [];
  }

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
      stop.innerHTML = stopping ? '<span class="loading loading-spinner loading-xs" aria-hidden="true"></span> Stopping…' : 'Stop';
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
      text.textContent = statusLine({
        phase: state.phase,
        elapsed: state.started ? (Date.now() - state.started) / 1000 : 0,
        activity: state.activity,
        background: state.mode === 'poll',
      });
    }
  }

  var HEADER_LABEL = {
    working: 'Assistant is working…',
    waiting: 'The assistant is waiting for your approval',
    unread: 'The assistant has a new reply',
    '': 'Ask the assistant',
  };

  /** The header Assistant button: working ring, approval color, unread dot. */
  function paintHeader() {
    var st = state.phase === 'waiting' ? 'waiting' : busy() ? 'working' : state.unread ? 'unread' : '';
    document.querySelectorAll('[data-neo-agent-launch]').forEach(function (b) {
      b.setAttribute('data-agent-state', st);
      b.title = HEADER_LABEL[st];
      b.setAttribute('aria-label', HEADER_LABEL[st]);
    });
  }

  /** Remember the running turn / the unread reply across reloads. */
  function persist() {
    storeSet(STORE_RUN, busy() && state.run && state.session ? {
      session_id: state.session,
      run_id: state.run,
      context: state.context,
      title: state.title,
      started: state.started,
    } : null);
    storeSet(STORE_UNREAD, state.unread && state.session ? {
      session_id: state.session,
      context: state.context,
      title: state.title,
    } : null);
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
        return r.ok ? r.json() : null;
      })
      .then(function (j) {
        if (!j || run !== state.run) return;
        state.lastRun = j;
        var pending = (j.approvals || []).map(function (a) { return a.request_id; });
        if (j.approvals_known) {
          state.approvals.slice().forEach(function (a) {
            if (a.open && !a.sending && a.id && pending.indexOf(a.id) < 0) {
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
            approvalCard(a);
          });
        }
        if (state.phase !== 'stopping') {
          setPhase(state.approvals.some(function (a) { return a.open; }) || j.status === 'waiting_for_approval' ? 'waiting' : 'working');
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
    state.liveRow = spinnerNote('Still working on the reply. It shows up here when it is done.');
  }

  function outcomeNote(status, error) {
    var o = outcomeText(status, error);
    if (o) note(o.text, o.error ? 'neo-agent-error' : 'neo-agent-end');
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
    endApprovals();
    setPhase('idle');
    if (state.liveRow) state.liveRow.remove();
    state.liveRow = null;
    // Also while the chat is still loading (its messages may predate the end).
    if (state.session && (state.loaded || state.isOpen)) {
      loadChat(state.session, { keepView: true }).then(function () { outcomeNote(j.status, j.error); });
    }
    markUnread();
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
    // Segments after a tool call start with blank lines; pre-wrap would show them.
    t.el.textContent = t.raw.replace(/^\s+/, '');
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
    var failed = g.lines.filter(function (l) { return l.status === 'error'; }).length;
    var open = g.lines.filter(function (l) { return l.status === 'running'; });
    if (running && open.length) {
      g.sum.innerHTML = '<span class="loading loading-dots loading-xs"></span> ';
      g.sum.appendChild(document.createTextNode(toolLabel(open[open.length - 1].name)));
    } else {
      g.sum.textContent =
        (failed ? '⚠ ' : '✓ ') + 'Used ' + g.lines.length + (g.lines.length === 1 ? ' tool' : ' tools') +
        (failed ? ' (' + failed + ' failed)' : '');
    }
  };

  var STATUS_ICON = { running: '…', done: '✓', error: '✗', stopped: '–' };

  /** Summary row of one tool call: icon, plain label, duration. */
  function paintLine(line) {
    var s = line.sum;
    s.textContent = '';
    var icon = document.createElement('span');
    icon.className = 'neo-agent-tool-icon neo-agent-tool-icon--' + line.status;
    icon.textContent = STATUS_ICON[line.status] || '✓';
    s.appendChild(icon);
    s.appendChild(document.createTextNode(toolLabel(line.name)));
    var d = toolDetail(line);
    if (d.duration) {
      var t = document.createElement('span');
      t.className = 'neo-agent-tool-time';
      t.textContent = d.duration;
      s.appendChild(t);
    }
    if (line.el.open) paintDetail(line);
  }

  /** Expanded body: tool name, pretty input, output (filled lazily on open). */
  function paintDetail(line) {
    var d = toolDetail(line);
    var body = line.body;
    body.textContent = '';
    var meta = document.createElement('div');
    meta.className = 'neo-agent-tool-meta';
    meta.textContent = d.meta;
    body.appendChild(meta);
    d.sections.forEach(function (sec) {
      var label = document.createElement('div');
      label.className = 'neo-agent-tool-label';
      label.textContent = sec.label;
      body.appendChild(label);
      var v = document.createElement(sec.pre ? 'pre' : 'div');
      v.className = sec.pre ? 'neo-agent-tool-pre' : sec.mono ? 'neo-agent-tool-name' : 'neo-agent-tool-note';
      v.textContent = sec.text;
      body.appendChild(v);
    });
  }

  /** A collapsible tool call inside group `g`. */
  function toolLine(g, data) {
    var el = document.createElement('details');
    el.className = 'neo-agent-tool';
    var sum = document.createElement('summary');
    var body = document.createElement('div');
    body.className = 'neo-agent-tool-body';
    el.appendChild(sum);
    el.appendChild(body);
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
    scrollDown();
  };

  Turn.prototype.toolDone = function (d, failed) {
    var line = null;
    for (var i = 0; i < this.lines.length; i++) {
      if (this.lines[i].status === 'running' && this.lines[i].name === d.tool_name) { line = this.lines[i]; break; }
    }
    if (!line) return;
    line.status = failed ? 'error' : 'done';
    line.ended = d.ts;
    paintLine(line);
    this.summarize(line.group, true);
  };

  /** Results from the turn transcript: nth call of a tool ↔ nth line of it. */
  Turn.prototype.results = function (calls) {
    var used = [];
    this.lines.forEach(function (line) {
      for (var i = 0; i < calls.length; i++) {
        if (used[i] || calls[i].name !== line.name) continue;
        used[i] = true;
        if (calls[i].result !== undefined) line.result = calls[i].result;
        if (line.args == null && calls[i].args != null) line.args = calls[i].args;
        break;
      }
    });
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
      if (t.raw.trim()) chain = chain.then(function () { return renderInto(t.el, t.raw); });
      else t.el.remove();
    });
    return chain;
  };

  function jsonOrThrow(r) {
    return r.json().then(
      function (j) {
        if (!r.ok) throw new Error((j && j.error) || 'The assistant is not reachable.');
        return j;
      },
      function () {
        throw new Error('The assistant is not reachable (' + r.status + ').');
      }
    );
  }

  function runningLines(turn) {
    return turn.lines.filter(function (l) { return l.status === 'running'; });
  }

  function send(text) {
    text = (text || '').trim();
    if (!text || busy() || !state.session) return;
    if (state.view !== 'chat') showView('chat');
    userBubble(text);
    state.empty = false;
    var turn = new Turn(el('neo-agent-log'));
    var waiting = turn.append(document.createElement('div'));
    waiting.className = 'neo-agent-msg neo-agent-msg--note';
    waiting.innerHTML = '<span class="loading loading-dots loading-xs"></span>';
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
          var open = runningLines(turn);
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

  // ---- chats: new, reopen, history list -------------------------------------

  function setTitle(t) {
    state.title = t || 'Assistant';
    var h = el('neo-agent-title');
    if (h) h.textContent = state.title;
  }

  function clearLog() {
    endApprovals();
    state.liveRow = null;
    var log = el('neo-agent-log');
    if (log) log.innerHTML = '';
  }

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
    return post('/agent/open', { context: state.context, fresh: true })
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

  var TRASH_SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4" aria-hidden="true"><path fill-rule="evenodd" d="M8.75 1A2.75 2.75 0 006 3.75v.443c-.795.077-1.584.176-2.365.298a.75.75 0 10.23 1.482l.149-.022.841 10.518A2.75 2.75 0 007.596 19h4.807a2.75 2.75 0 002.742-2.53l.841-10.52.149.023a.75.75 0 00.23-1.482A41.03 41.03 0 0014 4.193V3.75A2.75 2.75 0 0011.25 1h-2.5zM10 4c.84 0 1.673.025 2.5.075V3.75c0-.69-.56-1.25-1.25-1.25h-2.5c-.69 0-1.25.56-1.25 1.25v.325C8.327 4.025 9.16 4 10 4zM8.58 7.72a.75.75 0 00-1.5.06l.3 7.5a.75.75 0 101.5-.06l-.3-7.5zm4.34.06a.75.75 0 10-1.5-.06l-.3 7.5a.75.75 0 101.5.06l.3-7.5z" clip-rule="evenodd"/></svg>';

  function showHistory() {
    showView('history');
    var box = el('neo-agent-history');
    if (!box) return;
    box.textContent = '';
    var loading = document.createElement('div');
    loading.className = 'neo-agent-msg neo-agent-msg--note neo-agent-loading';
    loading.innerHTML = '<span class="loading loading-spinner loading-xs" aria-hidden="true"></span> Loading your chats…';
    box.appendChild(loading);
    fetch('/agent/sessions?context=' + encodeURIComponent(state.context), { credentials: 'same-origin' })
      .then(jsonOrThrow)
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
    var top = document.createElement('div');
    top.className = 'flex items-center gap-2 mb-2';
    var back = document.createElement('button');
    back.type = 'button';
    back.className = 'btn btn-ghost btn-xs';
    back.textContent = '← Back to the chat';
    back.addEventListener('click', backToChat);
    top.appendChild(back);
    box.appendChild(top);
    var groups = groupSessions(items, state.context);
    if (!groups.length) {
      var none = document.createElement('p');
      none.className = 'neo-agent-msg neo-agent-msg--note';
      none.textContent = 'No earlier chats yet. A chat shows up here after its first message.';
      box.appendChild(none);
      return;
    }
    groups.forEach(function (g) {
      var h = document.createElement('h3');
      h.className = 'neo-agent-hist-head';
      h.textContent = g.label;
      box.appendChild(h);
      var ul = document.createElement('ul');
      ul.className = 'neo-agent-hist-list';
      g.items.forEach(function (s) {
        ul.appendChild(historyRow(s, items));
      });
      box.appendChild(ul);
    });
  }

  function historyRow(s, all) {
    var li = document.createElement('li');
    var current = s.id === state.session;
    li.className = 'neo-agent-hist-row' + (current ? ' is-current' : '');
    var openBtn = document.createElement('button');
    openBtn.type = 'button';
    openBtn.className = 'neo-agent-hist-open';
    var ico = document.createElement('span');
    ico.className = 'neo-agent-ref-ico';
    ico.setAttribute('aria-hidden', 'true');
    if (s.icon) {
      var img = document.createElement('img');
      img.src = s.icon;
      img.alt = '';
      ico.appendChild(img);
    } else {
      ico.textContent = s.context && s.context.indexOf('guide:') === 0 ? '☰' : s.context === 'general' ? '✦' : '▣';
    }
    openBtn.appendChild(ico);
    var main = document.createElement('span');
    main.className = 'min-w-0 flex-1';
    var title = document.createElement('span');
    title.className = 'block text-sm font-medium truncate';
    title.textContent = s.title || s.preview || 'Chat';
    var meta = document.createElement('span');
    meta.className = 'block text-xs text-base-content/60 truncate';
    var bits = [s.label, relativeTime(s.last_active)];
    if (s.message_count) bits.push(s.message_count + (s.message_count === 1 ? ' message' : ' messages'));
    meta.textContent = bits.filter(Boolean).join(' · ');
    main.appendChild(title);
    main.appendChild(meta);
    openBtn.appendChild(main);
    if (current) {
      var badge = document.createElement('span');
      badge.className = 'badge badge-xs shrink-0 ' + (busy() ? 'badge-primary' : 'badge-ghost');
      badge.textContent = busy() ? 'Working…' : 'Open';
      openBtn.appendChild(badge);
    }
    var locked = busy() && !current;
    if (locked) {
      openBtn.disabled = true;
      openBtn.title = 'Wait for the current reply or stop it first';
    }
    openBtn.addEventListener('click', function () {
      if (current && state.loaded) backToChat();
      else loadChat(s.id, { title: s.title || s.label });
    });
    li.appendChild(openBtn);
    var del = document.createElement('button');
    del.type = 'button';
    del.className = 'btn btn-ghost btn-xs btn-square shrink-0';
    del.setAttribute('aria-label', 'Delete this chat');
    del.title = 'Delete this chat';
    del.innerHTML = TRASH_SVG;
    del.disabled = current && busy();
    del.addEventListener('click', function () {
      confirmDelete(li, s, all);
    });
    li.appendChild(del);
    return li;
  }

  /** Delete with the question in the row itself (no browser dialog). */
  function confirmDelete(li, s, all) {
    var kept = Array.prototype.slice.call(li.childNodes);
    li.textContent = '';
    li.classList.add('is-confirm');
    var q = document.createElement('span');
    q.className = 'text-sm flex-1 min-w-0';
    q.textContent = 'Delete “' + (s.title || 'this chat') + '”? This cannot be undone.';
    var yes = document.createElement('button');
    yes.type = 'button';
    yes.className = 'btn btn-error btn-xs';
    yes.textContent = 'Delete';
    var no = document.createElement('button');
    no.type = 'button';
    no.className = 'btn btn-ghost btn-xs';
    no.textContent = 'Cancel';
    no.addEventListener('click', function () {
      li.textContent = '';
      li.classList.remove('is-confirm');
      kept.forEach(function (n) { li.appendChild(n); });
    });
    yes.addEventListener('click', function () {
      yes.disabled = true;
      no.disabled = true;
      yes.innerHTML = '<span class="loading loading-spinner loading-xs" aria-hidden="true"></span> Deleting…';
      fetch('/agent/sessions/' + encodeURIComponent(s.id), { method: 'DELETE', credentials: 'same-origin' })
        .then(jsonOrThrow)
        .then(function () {
          if (s.id === state.session && !busy()) {
            state.session = null;
            state.loaded = false;
            state.unread = false;
            clearLog();
            persist();
            paintHeader();
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
    showView('chat');
    if (!state.session && !busy()) startNew(state.context, { title: state.defaultTitle, hint: state.hint || undefined });
    else if (state.session && !state.loaded) loadChat(state.session, { title: state.title });
    focusInput();
  }

  function toggleHistory() {
    if (state.view === 'history') backToChat();
    else showHistory();
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

  function focusInput() {
    var input = el('neo-agent-input');
    if (input && state.view === 'chat') setTimeout(function () { input.focus(); }, 50);
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
    var saved = parseSavedRun(storeGet(STORE_RUN));
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
    var unread = parseSavedRun(storeGet(STORE_UNREAD));
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

  document.addEventListener('DOMContentLoaded', function () {
    var form = el('neo-agent-form');
    if (!form) return;
    var input = el('neo-agent-input');
    var picker = refPicker(input, el('neo-agent-refs'));
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
    history: toggleHistory,
    newChat: function () {
      if (busy()) return;
      startNew(state.context, {
        title: state.defaultTitle || state.title,
        starter: state.starter || undefined,
        hint: state.hint || undefined,
      });
    },
  };
})(typeof window !== 'undefined' ? window : globalThis);
