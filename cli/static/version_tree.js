// version_tree.js — pure helpers for the Versioning timeline (versioning.js):
// lane geometry, SVG edge paths, day grouping and change summaries. The lane
// assignment itself happens server-side (cli/src/commands/web/version_tree).
// Tested by version_tree.test.js (`just test-widgets`).
(function (root) {
  'use strict';

  /** Gutter geometry for a graph with `lanes` lanes. */
  function metrics(lanes, compact) {
    if (!lanes) return { pad: 0, lw: 0, width: 0 };
    var pad = compact ? 7 : 11;
    var maxW = compact ? 64 : 104;
    var lw = Math.max(7, Math.min(compact ? 11 : 15, (maxW - 2 * pad) / lanes));
    return { pad: pad, lw: lw, width: Math.round(2 * pad + lanes * lw) };
  }

  /** Lane centre. `mirror`: lane 0 hugs the right edge (system gutter). */
  function laneX(m, lane, mirror) {
    var x = m.pad + lane * m.lw + m.lw / 2;
    return mirror ? m.width - x : x;
  }

  function r1(n) {
    return Math.round(n * 10) / 10;
  }

  /**
   * SVG path for an edge from (x1, y1) down to (x2, y2) travelling in column
   * xl: bend from x1 into xl right below the child, straight down, bend into
   * x2 right above the parent. Bends are S-curves at most `bend` px tall.
   */
  function edgePath(x1, y1, xl, x2, y2, bend) {
    var span = Math.max(0, y2 - y1);
    var needStart = x1 !== xl;
    var needEnd = x2 !== xl;
    var b = Math.min(bend || 18, needStart && needEnd ? span / 2 : span);
    var d = 'M' + r1(x1) + ',' + r1(y1);
    var y = y1;
    if (needStart) {
      var ys = y1 + b;
      d += ' C' + r1(x1) + ',' + r1(y1 + b / 2) + ' ' + r1(xl) + ',' + r1(y1 + b / 2) + ' ' + r1(xl) + ',' + r1(ys);
      y = ys;
    }
    if (needEnd) {
      var ye = y2 - b;
      if (ye > y) d += ' L' + r1(xl) + ',' + r1(ye);
      d += ' C' + r1(xl) + ',' + r1(ye + b / 2) + ' ' + r1(x2) + ',' + r1(ye + b / 2) + ' ' + r1(x2) + ',' + r1(y2);
    } else if (y2 > y) {
      d += ' L' + r1(xl) + ',' + r1(y2);
    }
    return d;
  }

  function dayKey(ts) {
    var d = new Date(ts * 1000);
    return d.getFullYear() + '-' + (d.getMonth() + 1) + '-' + d.getDate();
  }

  function pad2(n) {
    return n < 10 ? '0' + n : String(n);
  }

  /** Local HH:MM for a unix timestamp (seconds). */
  function clockTime(ts) {
    if (!ts) return '';
    var d = new Date(ts * 1000);
    return pad2(d.getHours()) + ':' + pad2(d.getMinutes());
  }

  /** Local YYYY-MM-DD HH:MM:SS for a unix timestamp (seconds). */
  function fullTime(ts) {
    if (!ts) return '';
    var d = new Date(ts * 1000);
    return (
      d.getFullYear() + '-' + pad2(d.getMonth() + 1) + '-' + pad2(d.getDate()) +
      ' ' + pad2(d.getHours()) + ':' + pad2(d.getMinutes()) + ':' + pad2(d.getSeconds())
    );
  }

  function sameCalendarDay(ts, nowTs) {
    if (!ts) return false;
    var d = new Date(ts * 1000);
    var n = new Date((nowTs != null ? nowTs : Date.now() / 1000) * 1000);
    return (
      d.getFullYear() === n.getFullYear() &&
      d.getMonth() === n.getMonth() &&
      d.getDate() === n.getDate()
    );
  }

  /** Relative age from `nowTs` (unix seconds; default now). */
  function relTime(ts, nowTs) {
    if (!ts) return '';
    var now = nowTs != null ? nowTs : Date.now() / 1000;
    var s = Math.max(0, now - ts);
    if (s < 60) return 'just now';
    var m = Math.floor(s / 60);
    if (m < 60) return m + (m === 1 ? ' minute ago' : ' minutes ago');
    var h = Math.floor(m / 60);
    if (h < 24) return h + (h === 1 ? ' hour ago' : ' hours ago');
    var d = Math.floor(h / 24);
    if (d < 30) return d + (d === 1 ? ' day ago' : ' days ago');
    var mo = Math.floor(d / 30);
    if (mo < 12) return mo + (mo === 1 ? ' month ago' : ' months ago');
    var y = Math.floor(d / 365);
    return y + (y === 1 ? ' year ago' : ' years ago');
  }

  /**
   * Collapsed timeline subtitle time: clock only on the same calendar day as
   * `nowTs`; otherwise relative age plus clock so age is visible without expand.
   */
  function quickTime(ts, nowTs) {
    if (!ts) return '';
    var c = clockTime(ts);
    if (sameCalendarDay(ts, nowTs)) return c;
    var rel = relTime(ts, nowTs);
    return rel ? rel + ' · ' + c : c;
  }

  /**
   * Interleave day separators with rows (newest first). Returns
   * `{ type: 'day', key, ts }` and `{ type: 'row', key, index, row }`.
   * Pending (unapplied) rows never start a day.
   */
  function withDays(rows) {
    var out = [];
    var last = null;
    (rows || []).forEach(function (row, index) {
      if (!row.pending) {
        var k = dayKey(row.time);
        if (k !== last) {
          out.push({ type: 'day', key: 'day-' + k, ts: row.time });
          last = k;
        }
      }
      out.push({ type: 'row', key: 'row-' + (row.key || index), index: index, row: row });
    });
    return out;
  }

  function listText(names, max) {
    max = max || 2;
    if (names.length <= max) return names.join(', ');
    return names.slice(0, max).join(', ') + ' +' + (names.length - max);
  }

  /** One-line summary of a commit's settings change, or '' when unknown. */
  function changeText(ch) {
    if (!ch) return '';
    if (ch.initial) return 'First version · ' + ch.enabled + (ch.enabled === 1 ? ' service' : ' services');
    if (ch.unchanged) return 'No settings changes';
    var parts = [];
    if (ch.added && ch.added.length) parts.push('Enabled ' + listText(ch.added));
    if (ch.removed && ch.removed.length) parts.push('Disabled ' + listText(ch.removed));
    var changed = (ch.changed || []).concat(ch.sections || []);
    if (changed.length) parts.push('Changed ' + listText(changed));
    if (ch.inputs && ch.inputs.length) parts.push('Updated ' + listText(ch.inputs));
    return parts.join(' · ');
  }

  /** User note on a commit id / generation number from tree.notes, or ''. */
  function commitNote(notes, id) {
    return (notes && notes.commits && id && notes.commits[id]) || '';
  }

  function genNote(notes, n) {
    return (notes && notes.generations && n != null && notes.generations[String(n)]) || '';
  }

  /** Form body for POST /versioning/notes (`kind` commit | generation). */
  function noteBody(kind, id, note) {
    var p = new URLSearchParams();
    p.set('kind', kind);
    p.set('id', String(id));
    p.set('note', note == null ? '' : String(note));
    return p.toString();
  }

  var api = {
    metrics: metrics,
    laneX: laneX,
    edgePath: edgePath,
    dayKey: dayKey,
    clockTime: clockTime,
    fullTime: fullTime,
    sameCalendarDay: sameCalendarDay,
    relTime: relTime,
    quickTime: quickTime,
    withDays: withDays,
    changeText: changeText,
    commitNote: commitNote,
    genNote: genNote,
    noteBody: noteBody,
  };
  root.NeoVersionTree = api;
  if (typeof module === 'object' && module.exports) module.exports = api;
})(typeof globalThis !== 'undefined' ? globalThis : this);
