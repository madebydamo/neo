// versioning.js — D3 commit graph + selection actions for the Versioning tab.
// Loaded by configuration.html.hbs; re-inits after HTMX swaps via neoInitVersioning.

(function () {
  'use strict';

  var MAX_SELECT = 2;
  var ROW_H = 30;
  var COL_W = 18;
  var PAD_L = 20;
  var PAD_T = 18;
  var PAD_R = 24;
  var PAD_B = 20;
  var NODE_R = 5;
  var LABEL_GAP = 14;
  var BADGE_R = 8;
  /** Horizontal offset of A/B badge center from the commit node center. */
  var BADGE_DX = 16;

  var state = {
    graph: null,
    selected: [], // commit ids, max 2; order = diff a → b
    layout: null,
  };

  function $(id) {
    return document.getElementById(id);
  }

  function toast(msg, type) {
    if (typeof window.neoToast === 'function') {
      window.neoToast(msg, type || 'info');
    }
  }

  function short(id) {
    return id && id.length > 7 ? id.slice(0, 7) : id || '';
  }

  function pad2(n) {
    return n < 10 ? '0' + n : String(n);
  }

  /** 24h local time: `2026-07-15 16:09:01` */
  function formatTime(ts) {
    if (!ts) return '';
    try {
      var d = new Date(ts * 1000);
      if (isNaN(d.getTime())) return '';
      return (
        d.getFullYear() +
        '-' +
        pad2(d.getMonth() + 1) +
        '-' +
        pad2(d.getDate()) +
        ' ' +
        pad2(d.getHours()) +
        ':' +
        pad2(d.getMinutes()) +
        ':' +
        pad2(d.getSeconds())
      );
    } catch (e) {
      return String(ts);
    }
  }

  /**
   * `activation_20260721-123033` → `2026-07-21 12:30:33`
   * Falls back to the raw name if the pattern does not match.
   */
  function formatActivationName(name) {
    if (!name) return '';
    var m = /^activation_(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})$/.exec(name);
    if (m) {
      return m[1] + '-' + m[2] + '-' + m[3] + ' ' + m[4] + ':' + m[5] + ':' + m[6];
    }
    return name;
  }

  /** Prefer human activation time from branch name; else commit timestamp. */
  function commitDisplayTime(c) {
    if (c && c.branches && c.branches.length) {
      for (var i = 0; i < c.branches.length; i++) {
        var pretty = formatActivationName(c.branches[i]);
        if (pretty !== c.branches[i]) return pretty;
      }
    }
    if (c && c.subject) {
      var m = /activation_(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})/.exec(c.subject);
      if (m) {
        return m[1] + '-' + m[2] + '-' + m[3] + ' ' + m[4] + ':' + m[5] + ':' + m[6];
      }
    }
    return formatTime(c && c.timestamp);
  }

  /**
   * Kind/title only — no date or generation (those are shown once elsewhere).
   * e.g. `Activation: activation_… (generation 214)` → `Activation`
   */
  function commitDisplaySubject(c) {
    if (!c || !c.subject) return '';
    var s = c.subject.trim();
    if (/^Activation:/i.test(s)) return 'Activation';
    if (/^Build:/i.test(s)) return 'Build';
    // Strip parenthetical generation and activation_ tokens if present.
    s = s.replace(/\s*\(generation\s+\d+\)\s*/gi, ' ');
    s = s.replace(/activation_\d{8}-\d{6}/gi, '');
    return s.replace(/\s+/g, ' ').trim() || c.subject;
  }

  /**
   * Lane-assign commits (API order: newest first) into a compact DAG layout.
   * Older commits inherit the lane of the newest child that lists them as first parent.
   */
  function layoutCommits(commits) {
    if (!commits || !commits.length) {
      return { nodes: [], edges: [], width: 0, height: 0, maxLane: 0 };
    }

    var laneOf = {};
    var nextLane = 0;
    commits.forEach(function (c, row) {
      var inherited = null;
      for (var i = 0; i < row; i++) {
        if (
          commits[i].parents &&
          commits[i].parents[0] === c.id &&
          laneOf[commits[i].id] != null
        ) {
          inherited = laneOf[commits[i].id];
          break;
        }
      }
      laneOf[c.id] = inherited != null ? inherited : nextLane++;
    });

    var nodes = commits.map(function (c, row) {
      var lane = laneOf[c.id];
      return {
        commit: c,
        row: row,
        lane: lane,
        x: PAD_L + lane * COL_W,
        y: PAD_T + row * ROW_H,
      };
    });

    var nodeById = {};
    nodes.forEach(function (n) {
      nodeById[n.commit.id] = n;
    });

    var edges = [];
    nodes.forEach(function (n) {
      (n.commit.parents || []).forEach(function (pid, pi) {
        var parent = nodeById[pid];
        if (!parent) return;
        edges.push({ from: n, to: parent, merge: pi > 0 });
      });
    });

    var maxLane = 0;
    nodes.forEach(function (n) {
      if (n.lane > maxLane) maxLane = n.lane;
    });

    var labelX = PAD_L + (maxLane + 1) * COL_W + LABEL_GAP;
    return {
      nodes: nodes,
      edges: edges,
      labelX: labelX,
      maxLane: maxLane,
      nodeById: nodeById,
      // Content height always fits every row (host may scroll).
      contentHeight: Math.max(120, PAD_T + commits.length * ROW_H + PAD_B),
    };
  }

  /** Center of the A/B selection badge for a laid-out node. */
  function badgeCenter(n) {
    return { x: n.x + BADGE_DX, y: n.y };
  }

  /** Point on circle at `from` toward `to`, inset by radius `r`. */
  function pointToward(from, to, r) {
    var dx = to.x - from.x;
    var dy = to.y - from.y;
    var len = Math.sqrt(dx * dx + dy * dy) || 1;
    return {
      x: from.x + (dx / len) * r,
      y: from.y + (dy / len) * r,
    };
  }

  /** Approximate mono label width (px) at ~10px font. */
  function approxTextWidth(text) {
    return Math.ceil(String(text || '').length * 6.4);
  }

  function isSelected(id) {
    return state.selected.indexOf(id) >= 0;
  }

  function selectionIndex(id) {
    return state.selected.indexOf(id);
  }

  function selectCommit(id, multi) {
    if (!id) return;
    if (multi) {
      var idx = state.selected.indexOf(id);
      if (idx >= 0) {
        state.selected.splice(idx, 1);
      } else if (state.selected.length === 0) {
        state.selected = [id];
      } else if (state.selected.length === 1) {
        state.selected.push(id);
      } else {
        // Keep the first (diff base); replace the second (diff target).
        state.selected[1] = id;
      }
    } else {
      if (state.selected.length === 1 && state.selected[0] === id) {
        state.selected = [];
      } else {
        state.selected = [id];
      }
    }
    renderDetail();
    renderDiff();
    paintSelection();
  }

  function clearSelection() {
    state.selected = [];
    renderDetail();
    renderDiff();
    paintSelection();
  }

  function paintSelection() {
    var host = $('versioning-graph-host');
    if (!host) return;

    var dual = state.selected.length === 2;

    host.querySelectorAll('.ver-row-bg').forEach(function (el) {
      var id = el.getAttribute('data-id');
      var idx = selectionIndex(id);
      el.setAttribute(
        'fill',
        idx === 0
          ? 'oklch(0.72 0.12 250 / 0.35)'
          : idx === 1
            ? 'oklch(0.75 0.14 145 / 0.35)'
            : 'transparent'
      );
      el.setAttribute(
        'stroke',
        idx >= 0 ? (idx === 0 ? 'oklch(0.55 0.14 250 / 0.45)' : 'oklch(0.55 0.14 145 / 0.45)') : 'none'
      );
      el.setAttribute('stroke-width', idx >= 0 ? '1' : '0');
    });

    host.querySelectorAll('.ver-node').forEach(function (el) {
      var id = el.getAttribute('data-id');
      var idx = selectionIndex(id);
      var on = idx >= 0;
      el.setAttribute('stroke-width', on ? '3' : '1.5');
      el.setAttribute('r', on ? NODE_R + 1.5 : NODE_R);
      if (on) {
        el.setAttribute(
          'stroke',
          idx === 0 ? 'oklch(0.55 0.18 250)' : 'oklch(0.55 0.18 145)'
        );
      } else {
        el.setAttribute('stroke', 'oklch(0.3 0.02 250)');
      }
    });

    host.querySelectorAll('.ver-label').forEach(function (el) {
      var id = el.getAttribute('data-id');
      var on = isSelected(id);
      el.setAttribute('font-weight', on ? '700' : '400');
      el.setAttribute('opacity', on ? '1' : '0.85');
    });

    var svg = host.querySelector('svg');
    if (!svg || typeof d3 === 'undefined') return;
    var gOverlay = d3.select(svg).select('g.ver-overlay');
    if (gOverlay.empty()) {
      gOverlay = d3.select(svg).append('g').attr('class', 'ver-overlay');
    }
    gOverlay.selectAll('*').remove();

    if (!state.layout || !state.layout.nodeById) return;

    // A/B badges + comparison arrow only when two commits are selected.
    if (!dual) return;

    var aNode = state.layout.nodeById[state.selected[0]];
    var bNode = state.layout.nodeById[state.selected[1]];
    if (!aNode || !bNode) return;

    var ba = badgeCenter(aNode);
    var bb = badgeCenter(bNode);

    // Arrow between badge centers (tip stops at badge edge).
    var start = pointToward(ba, bb, BADGE_R);
    var end = pointToward(bb, ba, BADGE_R + 1);

    gOverlay
      .append('defs')
      .append('marker')
      .attr('id', 'ver-diff-arrow')
      .attr('viewBox', '0 0 10 10')
      .attr('refX', 8)
      .attr('refY', 5)
      .attr('markerWidth', 7)
      .attr('markerHeight', 7)
      .attr('orient', 'auto')
      .append('path')
      .attr('d', 'M 0 0 L 10 5 L 0 10 z')
      .attr('fill', 'oklch(0.55 0.16 50)');

    var midX = (start.x + end.x) / 2;
    var midY = (start.y + end.y) / 2;
    // Slight bow to the right of the spine so the path sits on the badges, not under labels.
    var ctrlX = midX + 18;
    var ctrlY = midY;

    gOverlay
      .append('path')
      .attr(
        'd',
        'M' +
          start.x +
          ',' +
          start.y +
          ' Q' +
          ctrlX +
          ',' +
          ctrlY +
          ' ' +
          end.x +
          ',' +
          end.y
      )
      .attr('fill', 'none')
      .attr('stroke', 'oklch(0.55 0.16 50)')
      .attr('stroke-width', 2.25)
      .attr('stroke-dasharray', '5 3')
      .attr('marker-end', 'url(#ver-diff-arrow)')
      .attr('opacity', 0.95)
      .attr('class', 'ver-diff-link');

    gOverlay
      .append('text')
      .attr('x', ctrlX + 8)
      .attr('y', ctrlY + 3)
      .attr('font-size', '9px')
      .attr('font-weight', '600')
      .attr('fill', 'oklch(0.5 0.12 50)')
      .text('diff A → B');

    // Badges on top of the arrow endpoints.
    [
      { c: ba, letter: 'A', fill: 'oklch(0.55 0.18 250)' },
      { c: bb, letter: 'B', fill: 'oklch(0.55 0.18 145)' },
    ].forEach(function (item) {
      var g = gOverlay.append('g').attr('class', 'ver-sel-badge');
      g.append('circle')
        .attr('cx', item.c.x)
        .attr('cy', item.c.y)
        .attr('r', BADGE_R)
        .attr('fill', item.fill)
        .attr('stroke', 'oklch(1 0 0 / 0.85)')
        .attr('stroke-width', 1.25);
      g.append('text')
        .attr('x', item.c.x)
        .attr('y', item.c.y)
        .attr('text-anchor', 'middle')
        .attr('dominant-baseline', 'central')
        .attr('font-size', '9px')
        .attr('font-weight', '700')
        .attr('fill', 'white')
        .text(item.letter);
    });
  }

  function commitById(id) {
    if (!state.graph || !state.graph.commits) return null;
    for (var i = 0; i < state.graph.commits.length; i++) {
      if (state.graph.commits[i].id === id) return state.graph.commits[i];
    }
    return null;
  }

  function isBranchTip(c) {
    return c && c.branches && c.branches.length > 0;
  }

  /** Compact compare card: sha, kind · date once, gen once. */
  function selectionCardHtml(heading, tone, id, c) {
    var subj = commitDisplaySubject(c);
    var when = commitDisplayTime(c);
    var title = subj || 'Commit';
    if (when) title += ' · ' + when;
    var gen =
      c && c.generation != null
        ? '<div class="opacity-70">generation ' + c.generation + '</div>'
        : '';
    var toneBorder =
      tone === 'success' ? 'border-success/40 bg-success/10' : 'border-primary/40 bg-primary/10';
    var toneText = tone === 'success' ? 'text-success' : 'text-primary';
    return (
      '<div class="rounded border ' +
      toneBorder +
      ' px-2 py-1.5">' +
      '<div class="font-bold ' +
      toneText +
      '">' +
      escapeHtml(heading) +
      '</div>' +
      '<div class="font-mono opacity-60">' +
      escapeHtml(short(id)) +
      '</div>' +
      '<div>' +
      escapeHtml(title) +
      '</div>' +
      gen +
      '</div>'
    );
  }

  function postGenAction(n) {
    var url = '/versioning/generations/' + n + '/switch';
    var msg =
      'Switch the LIVE system to generation ' +
      n +
      ' via background job?\n\n' +
      'This runs outside the web UI (systemd-run) because the switch may restart neo-web. ' +
      'The page may disconnect — wait and reload if needed.';
    if (!window.confirm(msg)) return Promise.resolve();
    setStatus('<span class="opacity-50">Starting background generation job…</span>');
    return fetch(url, { method: 'POST', headers: { Accept: 'text/html' } })
      .then(function (r) {
        return r.text();
      })
      .then(function (html) {
        setStatus(html);
        var el = $('versioning-status');
        if (el && typeof htmx !== 'undefined' && htmx.process) {
          htmx.process(el);
        }
        toast('Generation switch started in background', 'info');
        // Refresh list later; switch may restart the UI first.
        setTimeout(function () {
          try {
            loadGenerations();
          } catch (e) {}
        }, 15000);
      })
      .catch(function (e) {
        setStatus(
          '<span class="text-error text-xs">' + escapeHtml(String(e)) + '</span>'
        );
        toast(String(e), 'error');
      });
  }

  function renderDetail() {
    var el = $('versioning-detail');
    var btnActivate = $('ver-btn-activate');
    if (!el) return;

    if (state.selected.length === 0) {
      el.innerHTML = '<p class="opacity-50 text-xs">Select a commit on the graph.</p>';
      if (btnActivate) btnActivate.disabled = true;
      return;
    }

    if (state.selected.length === 2) {
      var ca = commitById(state.selected[0]);
      var cb = commitById(state.selected[1]);
      el.innerHTML =
        '<p class="text-xs opacity-70 mb-2">Comparing settings.toml — direction <span class="font-mono font-semibold">A → B</span> (git diff A B).</p>' +
        '<div class="space-y-2 text-xs">' +
        selectionCardHtml('A · base', 'primary', state.selected[0], ca) +
        '<div class="text-center text-[10px] font-semibold opacity-50">↓ changes shown toward ↓</div>' +
        selectionCardHtml('B · compare', 'success', state.selected[1], cb) +
        '<p class="opacity-50 text-[10px]">Ctrl/Cmd-click another node to replace B (A stays fixed).</p>' +
        '</div>';
      if (btnActivate) btnActivate.disabled = true;
      return;
    }

    var id = state.selected[0];
    var c = commitById(id);
    if (!c) {
      el.innerHTML = '<p class="text-error text-xs">Unknown commit</p>';
      return;
    }
    var tip = isBranchTip(c);
    var html = '';
    html += '<div class="space-y-1">';
    html +=
      '<div class="font-mono text-xs"><span class="opacity-50">commit</span> ' +
      escapeHtml(short(c.id)) +
      (c.isHead ? ' <span class="badge badge-success badge-xs">HEAD</span>' : '') +
      '</div>';
    // One title, one date, one generation (in the actions box).
    var subj = commitDisplaySubject(c);
    var when = commitDisplayTime(c);
    if (subj) {
      html +=
        '<div class="text-sm font-medium">' +
        escapeHtml(subj) +
        (when ? ' · ' + escapeHtml(when) : '') +
        '</div>';
    } else if (when) {
      html += '<div class="text-sm font-medium">' + escapeHtml(when) + '</div>';
    }
    if (!tip) {
      html +=
        '<div class="text-[10px] opacity-40 mt-1">Not a branch tip — inspect/diff only</div>';
    }
    if (c.generation != null) {
      html +=
        '<div class="mt-2 rounded border border-base-300 bg-base-200/60 p-2 space-y-1.5">' +
        '<div class="text-xs">Generation <span class="font-mono font-bold text-base">' +
        c.generation +
        '</span></div>' +
        '<div class="flex flex-wrap gap-1">' +
        '<button type="button" class="btn btn-xs btn-warning ver-sel-gen-switch" data-n="' +
        c.generation +
        '">Switch now</button>' +
        '</div></div>';
    }
    html += '</div>';
    html +=
      '<div class="mt-2"><div class="text-[10px] font-semibold uppercase opacity-50 mb-1">Enabled services</div>' +
      '<div id="ver-services" class="flex flex-wrap gap-1 min-h-[1.5rem]"><span class="opacity-40 text-xs">Loading…</span></div></div>';
    el.innerHTML = html;

    if (btnActivate) btnActivate.disabled = !tip;

    el.querySelectorAll('.ver-sel-gen-switch').forEach(function (btn) {
      btn.addEventListener('click', function () {
        postGenAction(btn.getAttribute('data-n'));
      });
    });

    loadServices(c.id);
  }

  function loadServices(rev) {
    var host = $('ver-services');
    if (!host) return;
    fetch('/versioning/commit/' + encodeURIComponent(rev) + '/services')
      .then(function (r) {
        return r.json();
      })
      .then(function (data) {
        if (!host.isConnected) return;
        if (data.error) {
          host.innerHTML =
            '<span class="text-error text-xs">' + escapeHtml(data.error) + '</span>';
          return;
        }
        var en = data.enabled || [];
        if (!en.length) {
          host.innerHTML = '<span class="opacity-40 text-xs">None enabled</span>';
          return;
        }
        host.innerHTML = en
          .map(function (s) {
            return (
              '<span class="badge badge-sm badge-primary badge-outline">' +
              escapeHtml(s) +
              '</span>'
            );
          })
          .join('');
      })
      .catch(function (e) {
        if (host.isConnected) {
          host.innerHTML =
            '<span class="text-error text-xs">' + escapeHtml(String(e)) + '</span>';
        }
      });
  }

  function renderDiff() {
    var wrap = $('versioning-diff-wrap');
    var body = $('versioning-diff');
    var label = $('versioning-diff-label');
    if (!wrap || !body) return;
    if (state.selected.length !== 2) {
      wrap.classList.add('hidden');
      body.innerHTML = '';
      return;
    }
    wrap.classList.remove('hidden');
    var a = state.selected[0];
    var b = state.selected[1];
    if (label) {
      label.textContent = short(a) + ' → ' + short(b) + '  (A → B)';
    }
    body.innerHTML = '<p class="text-xs opacity-50 px-1">Loading diff…</p>';
    fetch(
      '/versioning/diff?a=' + encodeURIComponent(a) + '&b=' + encodeURIComponent(b)
    )
      .then(function (r) {
        return r.text();
      })
      .then(function (html) {
        body.innerHTML = html;
      })
      .catch(function (e) {
        body.innerHTML =
          '<div class="text-error text-sm">' + escapeHtml(String(e)) + '</div>';
      });
  }

  function escapeHtml(s) {
    return String(s == null ? '' : s)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  /** Graph row: sha · date · kind · gen — each field at most once. */
  function rowLabelText(c) {
    var parts = [short(c.id)];
    var t = commitDisplayTime(c);
    if (t) parts.push(t);
    var subj = commitDisplaySubject(c);
    if (subj) parts.push(subj);
    if (c.generation != null) parts.push('gen ' + c.generation);
    return parts.join('  ·  ');
  }

  function drawGraph() {
    var host = $('versioning-graph-host');
    var empty = $('versioning-graph-empty');
    if (!host) return;
    host.innerHTML = '';

    if (typeof d3 === 'undefined') {
      host.innerHTML =
        '<p class="p-4 text-error text-sm">D3 failed to load. Check network/CDN.</p>';
      return;
    }

    var commits = (state.graph && state.graph.commits) || [];
    if (!commits.length) {
      if (empty) empty.classList.remove('hidden');
      return;
    }
    if (empty) empty.classList.add('hidden');

    var layout = layoutCommits(commits);

    // Width: fill host when possible; expand if labels need more room (no mid-word cut).
    var hostW = Math.max(host.clientWidth || 0, 320);
    var maxLabelW = 0;
    layout.nodes.forEach(function (n) {
      maxLabelW = Math.max(maxLabelW, approxTextWidth(rowLabelText(n.commit)));
    });
    var contentW = layout.labelX + maxLabelW + PAD_R;
    var width = Math.max(hostW, contentW);
    var height = layout.contentHeight;

    layout.width = width;
    layout.height = height;
    state.layout = layout;

    var svg = d3
      .select(host)
      .append('svg')
      .attr('width', width)
      .attr('height', height)
      .attr('class', 'ver-graph-svg block')
      .attr('role', 'group');

    // Row backgrounds (selection highlight target)
    var gRows = svg.append('g').attr('class', 'ver-rows');
    layout.nodes.forEach(function (n) {
      gRows
        .append('rect')
        .attr('class', 'ver-row-bg')
        .attr('data-id', n.commit.id)
        .attr('x', 0)
        .attr('y', n.y - ROW_H / 2)
        .attr('width', width)
        .attr('height', ROW_H)
        .attr('rx', 2)
        .attr('fill', 'transparent');
    });

    // Edges
    var gEdges = svg.append('g').attr('class', 'ver-edges');
    layout.edges.forEach(function (e) {
      var x1 = e.from.x;
      var y1 = e.from.y;
      var x2 = e.to.x;
      var y2 = e.to.y;
      var midY = (y1 + y2) / 2;
      var d =
        'M' +
        x1 +
        ',' +
        y1 +
        ' C' +
        x1 +
        ',' +
        midY +
        ' ' +
        x2 +
        ',' +
        midY +
        ' ' +
        x2 +
        ',' +
        y2;
      gEdges
        .append('path')
        .attr('d', d)
        .attr('fill', 'none')
        .attr('stroke', e.merge ? 'oklch(0.65 0.15 250)' : 'oklch(0.55 0.02 250)')
        .attr('stroke-width', e.merge ? 1.5 : 1.25)
        .attr('opacity', 0.85);
    });

    // Nodes + labels
    var gNodes = svg.append('g').attr('class', 'ver-nodes');
    var labelX = layout.labelX;

    layout.nodes.forEach(function (n) {
      var c = n.commit;
      var g = gNodes.append('g').attr('class', 'ver-node-group').style('cursor', 'pointer');

      var fill = c.isHead
        ? 'oklch(0.72 0.17 145)'
        : isBranchTip(c)
          ? 'oklch(0.65 0.14 250)'
          : 'oklch(0.55 0.02 250)';

      g.append('circle')
        .attr('class', 'ver-node')
        .attr('data-id', c.id)
        .attr('cx', n.x)
        .attr('cy', n.y)
        .attr('r', NODE_R)
        .attr('fill', fill)
        .attr('stroke', 'oklch(0.3 0.02 250)')
        .attr('stroke-width', 1.5);

      var label = rowLabelText(c);

      g.append('text')
        .attr('class', 'ver-label')
        .attr('data-id', c.id)
        .attr('x', labelX)
        .attr('y', n.y + 4)
        .attr('font-size', '10px')
        .attr('font-family', 'ui-monospace, monospace')
        .attr('fill', 'currentColor')
        .attr('opacity', c.isHead ? 1 : 0.85)
        .text(label);

      // Hit target
      g.append('rect')
        .attr('x', 0)
        .attr('y', n.y - ROW_H / 2)
        .attr('width', width)
        .attr('height', ROW_H)
        .attr('fill', 'transparent')
        .on('click', function (event) {
          event.preventDefault();
          selectCommit(c.id, event.ctrlKey || event.metaKey);
        });
    });

    // Overlay group for selection chrome (must be on top)
    svg.append('g').attr('class', 'ver-overlay');

    paintSelection();
  }

  function setStatus(html) {
    var el = $('versioning-status');
    if (el) el.innerHTML = html || '';
  }

  function postAction(url, confirmMsg) {
    if (confirmMsg && !window.confirm(confirmMsg)) return Promise.resolve();
    setStatus('<span class="opacity-50">Working…</span>');
    return fetch(url, { method: 'POST', headers: { Accept: 'text/html' } })
      .then(function (r) {
        return r.text();
      })
      .then(function (html) {
        setStatus(html);
        return loadGraph().then(function () {
          if (html.indexOf('text-success') >= 0 || html.indexOf('activation') >= 0) {
            toast('Done', 'success');
          } else if (html.indexOf('text-error') >= 0) {
            toast('Action failed', 'error');
          }
        });
      })
      .catch(function (e) {
        setStatus(
          '<span class="text-error text-xs">' + escapeHtml(String(e)) + '</span>'
        );
        toast(String(e), 'error');
      });
  }

  function loadGraph() {
    return fetch('/versioning/graph')
      .then(function (r) {
        return r.json();
      })
      .then(function (data) {
        state.graph = data;
        drawGraph();
        renderDetail();
      })
      .catch(function (e) {
        var host = $('versioning-graph-host');
        if (host) {
          host.innerHTML =
            '<p class="p-4 text-error text-sm">Failed to load graph: ' +
            escapeHtml(String(e)) +
            '</p>';
        }
      });
  }

  function loadGenerations() {
    var host = $('versioning-generations');
    if (!host) return;
    host.innerHTML = '<p class="opacity-50 text-xs px-1">Loading…</p>';
    fetch('/versioning/generations')
      .then(function (r) {
        return r.json();
      })
      .then(function (data) {
        if (data.unavailable) {
          host.innerHTML =
            '<p class="opacity-50 text-xs px-1">' +
            escapeHtml(data.message || 'System generations unavailable.') +
            '</p>';
          return;
        }
        var gens = data.generations || [];
        if (!gens.length) {
          host.innerHTML =
            '<p class="opacity-50 text-xs px-1">No generations found.</p>';
          return;
        }
        var rows = gens
          .map(function (g) {
            var cur = g.isCurrent
              ? ' <span class="badge badge-success badge-xs">current</span>'
              : '';
            // Prefer server-provided 24h date string from nix-env
            var dateStr = g.date || '';
            return (
              '<tr class="hover:bg-base-200/50">' +
              '<td class="font-mono text-xs py-1 px-2">' +
              g.number +
              cur +
              '</td>' +
              '<td class="text-xs py-1 px-2 opacity-70 font-mono">' +
              escapeHtml(dateStr) +
              '</td>' +
              '<td class="py-1 px-2 text-right space-x-1">' +
              '<button type="button" class="btn btn-ghost btn-xs ver-gen-switch" data-n="' +
              g.number +
              '">Switch</button>' +
              '</td></tr>'
            );
          })
          .join('');
        host.innerHTML =
          '<div class="overflow-x-auto"><table class="table table-xs w-full">' +
          '<thead><tr><th>Gen</th><th>Date</th><th></th></tr></thead><tbody>' +
          rows +
          '</tbody></table></div>';

        host.querySelectorAll('.ver-gen-switch').forEach(function (btn) {
          btn.addEventListener('click', function () {
            postGenAction(btn.getAttribute('data-n'));
          });
        });
      })
      .catch(function (e) {
        host.innerHTML =
          '<p class="text-error text-xs px-1">' + escapeHtml(String(e)) + '</p>';
      });
  }

  function wireActions() {
    var btnActivate = $('ver-btn-activate');
    var btnClear = $('ver-btn-clear');
    var btnRefresh = $('ver-btn-refresh-gens');

    if (btnClear) {
      btnClear.onclick = clearSelection;
    }
    if (btnActivate) {
      btnActivate.onclick = function () {
        if (state.selected.length !== 1) return;
        var id = state.selected[0];
        postAction(
          '/versioning/activate/' + encodeURIComponent(id),
          'Activate this config (checkout branch + full nixos-rebuild)? This can take several minutes and creates a new system generation.'
        );
      };
    }
    if (btnRefresh) {
      btnRefresh.onclick = loadGenerations;
    }

    document.addEventListener('keydown', function onEsc(ev) {
      if (ev.key === 'Escape' && $('versioning-root')) {
        clearSelection();
      }
    });
  }

  var resizeTimer = null;
  function onHostResize() {
    if (!state.graph || !state.graph.commits || !state.graph.commits.length) return;
    if (resizeTimer) clearTimeout(resizeTimer);
    resizeTimer = setTimeout(function () {
      // Keep selection; only re-layout to fill width.
      var sel = state.selected.slice();
      drawGraph();
      state.selected = sel;
      renderDetail();
      renderDiff();
      paintSelection();
    }, 120);
  }

  window.neoInitVersioning = function neoInitVersioning() {
    var root = $('versioning-root');
    if (!root) return;
    state.selected = [];
    wireActions();
    loadGraph();
    loadGenerations();

    var host = $('versioning-graph-host');
    if (host && typeof ResizeObserver !== 'undefined') {
      if (host._verRo) {
        try {
          host._verRo.disconnect();
        } catch (e) {}
      }
      var ro = new ResizeObserver(onHostResize);
      ro.observe(host);
      host._verRo = ro;
    }
  };

  if (document.getElementById('versioning-root')) {
    window.neoInitVersioning();
  }
})();
