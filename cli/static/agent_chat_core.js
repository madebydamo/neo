// agent_chat_core.js — the assistant chat's logic without DOM access: SSE parsing,
// @ references, tool rows, time and run formatting. Tested by agent_chat.test.js.
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

  var KIND_LABEL = { service: 'App', option: 'Setting', guide: 'Guide', privacy: 'Privacy', finding: 'Finding' };

  var api = {
    KIND_LABEL: KIND_LABEL,
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
  root.NeoAgentCore = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})(typeof window !== 'undefined' ? window : globalThis);
