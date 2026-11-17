// versioning_core.js — what the Versioning timeline (versioning.js) says about
// its rows, without DOM or Alpine: titles and subtitles of versions and
// generations, which rows link to which, row highlighting, compare picks and
// day labels. Graph geometry and SVG are version_tree.js (loaded first).
// Tested by versioning_core.test.js.
(function (root, factory) {
  var VT = root && root.NeoVersionTree ? root.NeoVersionTree : typeof require === 'function' ? require('./version_tree.js') : null;
  var api = factory(VT);
  if (root) root.NeoVersioningCore = api;
  if (typeof module === 'object' && module.exports) module.exports = api;
})(typeof window !== 'undefined' ? window : typeof globalThis !== 'undefined' ? globalThis : null, function (VT) {
  'use strict';

  /**
   * Nearest first-parent ancestor of commit `c` that is an activation, else its
   * parent. `byId(id)` returns a loaded commit or null (the walk stops there).
   */
  function prevActivation(c, byId) {
    var parent = (c.parents || [])[0];
    var cur = parent;
    var guard = 0;
    while (cur && guard++ < 500) {
      var p = byId(cur);
      if (!p) break;
      if (p.kind === 'activation') return p.id;
      cur = (p.parents || [])[0];
    }
    return parent || null;
  }

  /** Row title: what changed in this version, else its kind. */
  function commitTitle(c) {
    var ch = c.changes;
    if (ch && ch.unchanged) {
      if (c.kind === 'activation') return 'Activated again · no settings changes';
    } else if (ch) {
      var t = VT.changeText(ch);
      if (t) return t;
    }
    if (c.kind === 'activation') return 'Activation';
    if (c.kind === 'build') return 'Build';
    return (c.subject || '').replace(/activation_\d{8}-\d{6}/gi, '').trim() || 'Commit';
  }

  function commitKindLabel(c) {
    if (c.kind === 'activation') return 'Activation';
    if (c.kind === 'build') return 'Build';
    return 'Commit';
  }

  /** Whether the row joins a commit and the generation it built. */
  function linked(row) {
    return !!(row.commit && row.system && row.system.type === 'generation');
  }

  function sysTitle(s) {
    if (s.type === 'switch') {
      return (s.rollback ? 'Rolled back to generation ' : 'Switched to generation ') + s.to;
    }
    return 'Generation ' + s.number;
  }

  function sysSubtitle(row) {
    var s = row.system;
    if (s.type === 'switch') {
      return s.from ? 'from generation ' + s.from + ' · builds continue from here' : 'builds continue from here';
    }
    var parts = [];
    if (s.identicalTo) parts.push('Identical to generation ' + s.identicalTo);
    else if (s.branched && s.parentGen) parts.push('Built on generation ' + s.parentGen + ' after a rollback');
    else if (row.commit) parts.push('New system');
    if (!row.commit && s.builtFrom) {
      parts.push('from version ' + s.builtFrom.short + (s.builtFrom.how === 'inferred' ? ' (likely)' : ''));
    } else if (!row.commit && !s.builtFrom) {
      parts.push('not linked to a saved version');
    }
    return parts.join(' · ');
  }

  /** Text for the system side of a commit row without its own generation. */
  function genStateText(c, systemAvailable) {
    if (!c.generation) {
      return c.kind === 'activation' && systemAvailable ? 'No system recorded' : '';
    }
    if (c.genState === 'same') return 'Same system as generation ' + c.generation;
    if (c.genState === 'missing') return 'Generation ' + c.generation + ' · deleted';
    if (c.genState === 'created') return 'Built generation ' + c.generation;
    return 'Generation ' + c.generation;
  }

  /**
   * What hovering `row` highlights in the other graph: { commit, gen } or null.
   * `running`: the running generation (status), for the running config's row.
   */
  function hoverLinks(row, running) {
    var hl = { commit: null, gen: null };
    var s = row.system;
    if (s && s.type === 'generation' && s.builtFrom) hl.commit = s.builtFrom.id;
    if (s && s.type === 'switch') hl.gen = s.to;
    if (row.commit && row.commit.generation && row.commit.genState === 'same') hl.gen = row.commit.generation;
    if (row.commit && row.commit.isRunningConfig && !s) {
      if (running) hl.gen = running;
    }
    return hl.commit || hl.gen ? hl : null;
  }

  /** Classes of a timeline row; `v` = { expanded, compare, hl, picks }. */
  function rowClass(row, v) {
    var cls = [];
    var hl = v.hl;
    if (v.expanded === row.key && !v.compare) cls.push('vt-open');
    if (hl && ((hl.commit && row.commit && row.commit.id === hl.commit) ||
        (hl.gen && row.system && row.system.type === 'generation' && row.system.number === hl.gen))) {
      cls.push('vt-hl');
    }
    if (v.compare && !row.commit) cls.push('opacity-40');
    if (v.compare && row.commit && (v.picks || []).indexOf(row.commit.id) >= 0) cls.push('vt-picked');
    return cls.join(' ');
  }

  function rowTime(row) {
    if (row.commit) return row.commit.timestamp;
    if (row.system) return row.system.type === 'switch' ? row.system.time : row.system.created;
    return 0;
  }

  /** Compare picks (A, B) after clicking `id`: unpick, add, or replace B. Mutates `picks`. */
  function togglePick(picks, id) {
    var i = picks.indexOf(id);
    if (i >= 0) picks.splice(i, 1);
    else if (picks.length < 2) picks.push(id);
    else picks.splice(1, 1, id);
    return picks;
  }

  /** Day header: "Today", "Yesterday", else a short local date (year when not this year). */
  function dayLabel(ts, nowMs) {
    var d = new Date(ts * 1000);
    var today = nowMs == null ? new Date() : new Date(nowMs);
    today.setHours(0, 0, 0, 0);
    var day = new Date(d.getTime());
    day.setHours(0, 0, 0, 0);
    var diff = Math.round((today - day) / 86400000);
    if (diff === 0) return 'Today';
    if (diff === 1) return 'Yesterday';
    var opts = { weekday: 'short', month: 'short', day: 'numeric' };
    if (d.getFullYear() !== today.getFullYear()) opts.year = 'numeric';
    return d.toLocaleDateString(undefined, opts);
  }

  return {
    prevActivation: prevActivation,
    commitTitle: commitTitle,
    commitKindLabel: commitKindLabel,
    linked: linked,
    sysTitle: sysTitle,
    sysSubtitle: sysSubtitle,
    genStateText: genStateText,
    hoverLinks: hoverLinks,
    rowClass: rowClass,
    rowTime: rowTime,
    togglePick: togglePick,
    dayLabel: dayLabel,
  };
});
