// versioning.js — Alpine component for the Versioning tab (branches.html.hbs).
//
// One timeline, two linked graphs: the configuration history (git commits of
// your settings; left gutter, circles) and the system lineage (NixOS
// generations; right gutter, squares). A commit and the generation it built
// share a row, joined by a "built" connector. Rows, lanes and links come from
// GET /versioning/tree (server-side lane assignment); this file measures the
// rendered rows and draws both gutters as inline SVG (helpers in
// version_tree.js). Diffs and job output open in the shared #changes-modal;
// the Data snapshots tab is the server-rendered ZFS card (/versioning/zfs).

window.versioningPage = function versioningPage() {
  var TAB_KEY = 'neo.versioningTab';
  var LEGEND_KEY = 'neo.versioningLegend';
  var PAGE = 40;
  var VT = window.NeoVersionTree;

  function pad2(n) {
    return n < 10 ? '0' + n : String(n);
  }

  function escapeHtml(s) {
    return String(s == null ? '' : s)
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
  }

  function toast(msg, type) {
    if (typeof window.neoToast === 'function') window.neoToast(msg, type || 'info');
  }

  function store(key, val) {
    try {
      if (val === undefined) return localStorage.getItem(key);
      localStorage.setItem(key, val);
    } catch (e) {}
    return null;
  }

  /** Output of a POST (activation monitor, alerts) in the shared dialog. */
  function openModal(title, html) {
    var m = document.getElementById('changes-modal');
    var body = document.getElementById('changes-body');
    if (!m || !body) return;
    var h = m.querySelector('h3');
    if (h) h.textContent = title;
    body.innerHTML = html;
    if (typeof htmx !== 'undefined' && htmx.process) htmx.process(body);
    if (!m.open) m.showModal();
  }

  var SPINNER = '<div class="flex justify-center py-8"><span class="loading loading-spinner loading-sm opacity-50"></span></div>';

  return {
    tab: 'timeline',
    loading: true,
    loadingMore: false,
    error: '',
    tree: null,
    limit: PAGE,
    expanded: null, // row key
    compare: false,
    picks: [], // commit ids (A, B)
    services: {}, // commit id → { enabled: [] } | { error } | null (loading)
    hl: null, // { commit, gen } linked to the hovered row
    legendOpen: true,
    zfsAvailable: false,
    cfgW: 0,
    sysW: 0,
    now: Date.now() / 1000,

    init() {
      var self = this;
      try {
        var t = sessionStorage.getItem(TAB_KEY);
        if (t === 'timeline' || t === 'data') this.tab = t;
      } catch (e) {}
      this.legendOpen = store(LEGEND_KEY) !== 'closed';
      this.loadTree();
      this.loadZfs();
      this._tick = setInterval(function () {
        if (!self.$root.isConnected) return clearInterval(self._tick);
        self.now = Date.now() / 1000;
      }, 60000);
      if (typeof ResizeObserver !== 'undefined') {
        this._ro = new ResizeObserver(function () {
          self.scheduleDraw();
        });
        this.$nextTick(function () {
          if (self.$refs.list) self._ro.observe(self.$refs.list);
        });
      }
      this._onResize = function () {
        self.scheduleDraw();
      };
      window.addEventListener('resize', this._onResize);
      if (document.fonts && document.fonts.ready) document.fonts.ready.then(this._onResize);
    },

    destroy() {
      clearInterval(this._tick);
      if (this._ro) this._ro.disconnect();
      window.removeEventListener('resize', this._onResize);
    },

    setTab(t) {
      this.tab = t;
      try {
        sessionStorage.setItem(TAB_KEY, t);
      } catch (e) {}
      if (t === 'timeline') this.scheduleDraw();
    },

    toggleLegend() {
      this.legendOpen = !this.legendOpen;
      store(LEGEND_KEY, this.legendOpen ? 'open' : 'closed');
    },

    onEscape() {
      if (document.querySelector('dialog[open]')) return;
      if (this.compare) this.toggleCompare();
      else this.expanded = null;
    },

    // ─── Data ──────────────────────────────────────────────────────────────

    loadTree() {
      var self = this;
      return fetch('/versioning/tree?limit=' + this.limit)
        .then(function (r) {
          return r.json();
        })
        .then(function (data) {
          if (data.error) throw new Error(data.error);
          self.tree = data;
          self.error = '';
          if (self.expanded && !self.rowByKey(self.expanded)) self.expanded = null;
        })
        .catch(function (e) {
          self.error = 'Failed to load history: ' + (e && e.message ? e.message : e);
        })
        .finally(function () {
          self.loading = false;
          self.loadingMore = false;
          self.scheduleDraw();
        });
    },

    refresh() {
      this.loading = !this.tree;
      this.loadTree();
      this.loadZfs();
    },

    showOlder() {
      this.limit += PAGE;
      this.loadingMore = true;
      return this.loadTree();
    },

    /** The ZFS card renders `class="hidden"` when the machine has no restore hook. */
    loadZfs() {
      var self = this;
      fetch('/versioning/zfs', { headers: { 'HX-Request': 'true' } })
        .then(function (r) {
          return r.text();
        })
        .then(function (html) {
          var host = self.$refs.zfsHost;
          if (!host) return;
          host.innerHTML = html;
          var card = host.firstElementChild;
          self.zfsAvailable = !!card && !card.classList.contains('hidden');
          if (!self.zfsAvailable && self.tab === 'data') self.tab = 'timeline';
          if (typeof htmx !== 'undefined' && htmx.process) htmx.process(host);
        })
        .catch(function () {
          self.zfsAvailable = false;
        });
    },

    loadServices(id) {
      if (!id || id in this.services) return;
      var self = this;
      this.services[id] = null;
      fetch('/versioning/commit/' + encodeURIComponent(id) + '/services')
        .then(function (r) {
          return r.json();
        })
        .then(function (data) {
          self.services[id] = data.error ? { error: data.error } : { enabled: data.enabled || [] };
        })
        .catch(function (e) {
          self.services[id] = { error: String(e) };
        })
        .finally(function () {
          self.scheduleDraw(); // the services line changes the row height
        });
    },

    svc(id) {
      return this.services[id] || null;
    },

    // ─── Derived ───────────────────────────────────────────────────────────

    rows() {
      return (this.tree && this.tree.rows) || [];
    },

    items() {
      return VT.withDays(this.rows());
    },

    status() {
      return (this.tree && this.tree.status) || { kind: 'unknown' };
    },

    systemAvailable() {
      return !!(this.tree && this.tree.system && this.tree.system.available);
    },

    rowByKey(key) {
      return this.rows().find(function (r) {
        return r.key === key;
      }) || null;
    },

    commitRow(id) {
      return this.rows().find(function (r) {
        return r.commit && r.commit.id === id;
      }) || null;
    },

    commitById(id) {
      var r = this.commitRow(id);
      return r ? r.commit : null;
    },

    genRow(n) {
      return this.rows().find(function (r) {
        return r.system && r.system.type === 'generation' && r.system.number === n;
      }) || null;
    },

    headCommit() {
      return this.tree ? this.commitById(this.tree.head) : null;
    },

    /** Nearest first-parent ancestor that is an activation (loaded), else the parent. */
    prevFor(c) {
      var parent = (c.parents || [])[0];
      var cur = parent;
      var guard = 0;
      while (cur && guard++ < 500) {
        var p = this.commitById(cur);
        if (!p) break;
        if (p.kind === 'activation') return p.id;
        cur = (p.parents || [])[0];
      }
      return parent || null;
    },

    /** Row title: what changed in this version, else its kind. */
    commitTitle(c) {
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
    },

    commitKindLabel(c) {
      if (c.kind === 'activation') return 'Activation';
      if (c.kind === 'build') return 'Build';
      return 'Commit';
    },

    /** Whether the row joins a commit and the generation it built. */
    linked(row) {
      return !!(row.commit && row.system && row.system.type === 'generation');
    },

    sysTitle(s) {
      if (s.type === 'switch') {
        return (s.rollback ? 'Rolled back to generation ' : 'Switched to generation ') + s.to;
      }
      return 'Generation ' + s.number;
    },

    sysSubtitle(row) {
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
    },

    /** Text for the system side of a commit row without its own generation. */
    genNote(c) {
      if (!c.generation) {
        return c.kind === 'activation' && this.systemAvailable() ? 'No system recorded' : '';
      }
      if (c.genState === 'same') return 'Same system as generation ' + c.generation;
      if (c.genState === 'missing') return 'Generation ' + c.generation + ' · deleted';
      if (c.genState === 'created') return 'Built generation ' + c.generation;
      return 'Generation ' + c.generation;
    },

    rowClass(row) {
      var cls = [];
      if (this.expanded === row.key && !this.compare) cls.push('vt-open');
      if (this.hl && ((this.hl.commit && row.commit && row.commit.id === this.hl.commit) ||
          (this.hl.gen && row.system && row.system.type === 'generation' && row.system.number === this.hl.gen))) {
        cls.push('vt-hl');
      }
      if (this.compare && !row.commit) cls.push('opacity-40');
      if (this.compare && row.commit && this.pickIndex(row.commit.id) >= 0) cls.push('vt-picked');
      return cls.join(' ');
    },

    rowPad() {
      return 'padding-left:' + this.cfgW + 'px;padding-right:' + this.sysW + 'px';
    },

    /** Hovering a row highlights what it is linked to in the other graph. */
    hover(row) {
      var hl = { commit: null, gen: null };
      var s = row.system;
      if (s && s.type === 'generation' && s.builtFrom) hl.commit = s.builtFrom.id;
      if (s && s.type === 'switch') hl.gen = s.to;
      if (row.commit && row.commit.generation && row.commit.genState === 'same') hl.gen = row.commit.generation;
      if (row.commit && row.commit.isRunningConfig && !s) {
        var st = this.status();
        if (st.running) hl.gen = st.running;
      }
      this.hl = hl.commit || hl.gen ? hl : null;
    },

    // ─── Graph drawing ─────────────────────────────────────────────────────

    scheduleDraw() {
      // rAF batches redraws; the timer covers hidden tabs where rAF pauses.
      var self = this;
      if (this._drawPending) return;
      this._drawPending = true;
      var run = function () {
        if (!self._drawPending) return;
        self._drawPending = false;
        self.$nextTick(function () {
          self.drawGraphs();
        });
      };
      requestAnimationFrame(run);
      setTimeout(run, 80);
    },

    drawGraphs() {
      var list = this.$refs.list;
      var cfgSvg = this.$refs.cfgSvg;
      var sysSvg = this.$refs.sysSvg;
      if (!list || !cfgSvg || !sysSvg || !this.tree || !list.offsetParent) return;
      var tree = this.tree;
      var box = list.getBoundingClientRect();
      var compact = box.width < 560;
      var cm = VT.metrics(tree.configGraph.lanes, compact);
      var sm = VT.metrics(this.systemAvailable() ? tree.systemGraph.lanes : 0, compact);
      if (cm.width !== this.cfgW || sm.width !== this.sysW) {
        // Row padding changes → rows reflow → the ResizeObserver redraws.
        this.cfgW = cm.width;
        this.sysW = sm.width;
        this.scheduleDraw();
        return;
      }
      var H = list.offsetHeight;
      var cfgY = {};
      var sysY = {};
      list.querySelectorAll('[data-row]').forEach(function (li) {
        var i = +li.getAttribute('data-row');
        var a = li.querySelector('[data-cfg-anchor]');
        var b = li.querySelector('[data-sys-anchor]');
        if (a) {
          var ra = a.getBoundingClientRect();
          cfgY[i] = ra.top - box.top + ra.height / 2;
        }
        if (b) {
          var rb = b.getBoundingClientRect();
          sysY[i] = rb.top - box.top + rb.height / 2;
        }
      });
      var rows = this.rows();
      var self = this;

      function edges(graph, ys, m, mirror, pendingRow) {
        var out = '';
        (graph.edges || []).forEach(function (e) {
          var y1 = ys[e.fromRow];
          if (y1 == null) return;
          var open = e.toRow == null;
          var y2 = open ? H - 2 : ys[e.toRow];
          if (y2 == null) return;
          var x1 = VT.laneX(m, e.fromLane, mirror);
          var xl = VT.laneX(m, e.lane, mirror);
          var x2 = VT.laneX(m, e.toLane, mirror);
          var cls = 'vt-edge' + (e.lane > 0 ? ' vt-edge-side' : '');
          if (pendingRow != null && e.fromRow === pendingRow) cls += ' vt-edge-pending';
          if (open) {
            var cut = Math.max(y1, y2 - 22);
            out += '<path class="' + cls + '" d="' + VT.edgePath(x1, y1, xl, xl, cut, 18) + '"/>';
            out += '<path class="' + cls + ' vt-edge-open" d="M' + xl + ',' + cut + ' L' + xl + ',' + y2 + '"/>';
          } else {
            out += '<path class="' + cls + '" d="' + VT.edgePath(x1, y1, xl, x2, y2, 18) + '"/>';
          }
        });
        return out;
      }

      var pendingRow = rows.length && rows[0].pending ? 0 : null;
      var cfgEdges = edges(tree.configGraph, cfgY, cm, false, pendingRow);
      var sysEdges = edges(tree.systemGraph, sysY, sm, true, null);
      var cfgNodes = '';
      var sysNodes = '';

      rows.forEach(function (row, i) {
        var open = self.expanded === row.key && !self.compare;
        if (row.pending && cfgY[i] != null) {
          var px = VT.laneX(cm, row.pendingLane || 0, false);
          cfgNodes += '<circle class="vt-dot-pending" cx="' + px + '" cy="' + cfgY[i] + '" r="5.5"/>';
        }
        var c = row.commit;
        if (c && cfgY[i] != null) {
          var x = VT.laneX(cm, c.lane, false);
          var y = cfgY[i];
          if (open) cfgNodes += '<circle class="vt-sel" cx="' + x + '" cy="' + y + '" r="12"/>';
          if (c.isRunningConfig) cfgNodes += '<circle class="vt-ring-run" cx="' + x + '" cy="' + y + '" r="9"/>';
          else if (c.isHead) cfgNodes += '<circle class="vt-ring-head" cx="' + x + '" cy="' + y + '" r="9"/>';
          if (c.kind === 'activation') {
            cfgNodes += '<circle class="vt-dot" cx="' + x + '" cy="' + y + '" r="' + (c.isHead ? 6 : 5) + '"/>';
          } else {
            cfgNodes += '<circle class="vt-dot-hollow" cx="' + x + '" cy="' + y + '" r="3.5"/>';
          }
        }
        var s = row.system;
        if (s && sysY[i] != null) {
          var sx = VT.laneX(sm, s.lane, true);
          var sy = sysY[i];
          if (open) sysNodes += '<circle class="vt-sel" cx="' + sx + '" cy="' + sy + '" r="12"/>';
          if (s.type === 'generation') {
            if (s.isRunning) {
              sysNodes += '<rect class="vt-halo" x="' + (sx - 6) + '" y="' + (sy - 6) + '" width="12" height="12" rx="3"/>';
            } else if (s.isBoot) {
              sysNodes += '<rect class="vt-ring-boot" x="' + (sx - 9) + '" y="' + (sy - 9) + '" width="18" height="18" rx="4.5"/>';
            }
            sysNodes += '<rect class="vt-sq' + (s.isRunning ? ' vt-sq-run' : '') + '" x="' + (sx - 5.5) + '" y="' + (sy - 5.5) +
              '" width="11" height="11" rx="2.5"/>';
          } else {
            var r = 5.5;
            sysNodes += '<path class="vt-diamond' + (s.isLatest ? ' vt-diamond-now' : '') + '" d="M' + sx + ',' + (sy - r) +
              ' L' + (sx + r) + ',' + sy + ' L' + sx + ',' + (sy + r) + ' L' + (sx - r) + ',' + sy + ' Z"/>';
          }
        }
      });

      cfgSvg.setAttribute('width', cm.width);
      cfgSvg.setAttribute('height', H);
      cfgSvg.innerHTML = cfgEdges + cfgNodes;
      sysSvg.setAttribute('width', sm.width);
      sysSvg.setAttribute('height', H);
      sysSvg.innerHTML = sysEdges + sysNodes;
    },

    // ─── Formatting ────────────────────────────────────────────────────────

    dayLabel(ts) {
      var d = new Date(ts * 1000);
      var today = new Date();
      today.setHours(0, 0, 0, 0);
      var day = new Date(d.getTime());
      day.setHours(0, 0, 0, 0);
      var diff = Math.round((today - day) / 86400000);
      if (diff === 0) return 'Today';
      if (diff === 1) return 'Yesterday';
      var opts = { weekday: 'short', month: 'short', day: 'numeric' };
      if (d.getFullYear() !== today.getFullYear()) opts.year = 'numeric';
      return d.toLocaleDateString(undefined, opts);
    },

    clock(ts) {
      return VT.clockTime(ts);
    },

    /** Collapsed row time: HH:MM today; "N days ago · HH:MM" when older. */
    quickTime(ts) {
      return VT.quickTime(ts, this.now);
    },

    fullTime(ts) {
      if (!ts) return '';
      var d = new Date(ts * 1000);
      return (
        d.getFullYear() + '-' + pad2(d.getMonth() + 1) + '-' + pad2(d.getDate()) +
        ' ' + pad2(d.getHours()) + ':' + pad2(d.getMinutes()) + ':' + pad2(d.getSeconds())
      );
    },

    relTime(ts) {
      return VT.relTime(ts, this.now);
    },

    rowTime(row) {
      if (row.commit) return row.commit.timestamp;
      if (row.system) return row.system.type === 'switch' ? row.system.time : row.system.created;
      return 0;
    },

    // ─── Interaction ───────────────────────────────────────────────────────

    onRow(row) {
      if (this.compare) {
        if (row.commit) this.togglePick(row.commit.id);
        return;
      }
      if (row.pending) {
        this.reviewPending();
        return;
      }
      this.expanded = this.expanded === row.key ? null : row.key;
      if (this.expanded && row.commit) {
        this.loadServices(row.commit.id);
      }
      this.scheduleDraw();
    },

    /** Expand a row by key and scroll to it; loads older pages when needed. */
    jumpTo(key, tries) {
      var self = this;
      var row = this.rowByKey(key);
      if (!row) {
        tries = tries || 0;
        if (this.tree && this.tree.hasMore && tries < 5) {
          this.showOlder().then(function () {
            self.jumpTo(key, tries + 1);
          });
        } else {
          toast('That entry is older than the loaded history.', 'info');
        }
        return;
      }
      this.compare = false;
      this.expanded = key;
      if (row.commit) this.loadServices(row.commit.id);
      this.scheduleDraw();
      this.$nextTick(function () {
        var el = self.$root.querySelector('[data-key="' + CSS.escape(key) + '"]');
        if (el) el.scrollIntoView({ block: 'center', behavior: 'smooth' });
      });
    },

    jumpToCommit(id) {
      var row = this.commitRow(id);
      this.jumpTo(row ? row.key : id);
    },

    jumpToGen(n) {
      var row = this.genRow(n);
      if (row) this.jumpTo(row.key);
      else this.jumpTo('g' + n);
    },

    toggleCompare() {
      this.compare = !this.compare;
      this.picks = [];
      this.scheduleDraw();
    },

    togglePick(id) {
      var i = this.picks.indexOf(id);
      if (i >= 0) this.picks.splice(i, 1);
      else if (this.picks.length < 2) this.picks.push(id);
      else this.picks.splice(1, 1, id);
    },

    pickIndex(id) {
      return this.picks.indexOf(id);
    },

    pickLabel(i) {
      var c = this.commitById(this.picks[i]);
      if (!c) return '';
      var d = new Date(c.timestamp * 1000);
      return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric' }) + ', ' + this.clock(c.timestamp) + ' · ' + c.short;
    },

    entryHeading(id, letter) {
      var c = this.commitById(id);
      if (!c) return '<span class="font-mono">' + escapeHtml(String(id).slice(0, 7)) + '</span>';
      return (
        '<div class="rounded-field border border-base-300 bg-base-200/50 px-3 py-2 min-w-0 flex-1">' +
        '<div class="text-[11px] font-semibold uppercase tracking-wider text-base-content/50">' + letter + '</div>' +
        '<div class="text-sm font-semibold truncate">' + escapeHtml(this.commitTitle(c)) +
        (c.generation ? ' <span class="font-normal text-base-content/60">· Generation ' + c.generation + '</span>' : '') +
        '</div>' +
        '<div class="text-xs text-base-content/60"><span>' + escapeHtml(this.fullTime(c.timestamp)) + '</span> · ' +
        '<span class="font-mono">' + escapeHtml(c.short) + '</span></div></div>'
      );
    },

    /** settings.toml diff a → b in the shared dialog. */
    showDiff(a, b, title) {
      var head =
        '<div class="flex flex-col sm:flex-row sm:items-center gap-2 mb-3">' +
        this.entryHeading(a, 'From') +
        '<span class="text-base-content/40 self-center rotate-90 sm:rotate-0" aria-hidden="true">→</span>' +
        this.entryHeading(b, 'To') +
        '</div>';
      openModal(title, head + SPINNER);
      fetch('/versioning/diff?a=' + encodeURIComponent(a) + '&b=' + encodeURIComponent(b))
        .then(function (r) {
          return r.text();
        })
        .then(function (html) {
          var body = document.getElementById('changes-body');
          if (!body) return;
          body.innerHTML = head + html;
          // Service links in the settings summary navigate via hx-get.
          if (typeof htmx !== 'undefined' && htmx.process) htmx.process(body);
        })
        .catch(function (e) {
          var body = document.getElementById('changes-body');
          if (body) body.innerHTML = head + '<div class="text-error text-sm">' + escapeHtml(String(e)) + '</div>';
        });
    },

    reviewPending() {
      openModal('Pending changes', SPINNER);
      fetch('/changes/summary')
        .then(function (r) {
          return r.text();
        })
        .then(function (html) {
          var body = document.getElementById('changes-body');
          if (!body) return;
          body.innerHTML = html;
          if (typeof htmx !== 'undefined' && htmx.process) htmx.process(body);
        });
    },

    post(url, title) {
      var self = this;
      return fetch(url, { method: 'POST', headers: { Accept: 'text/html' } })
        .then(function (r) {
          // 409: refused by an operation lock (toast, no dialog).
          if (r.status === 409 && window.NeoLocks) {
            return window.NeoLocks.handleFetch(r).then(function () {
              return null;
            });
          }
          return r.text();
        })
        .then(function (html) {
          if (html == null) return;
          openModal(title, html);
          self.loadTree();
        })
        .catch(function (e) {
          toast(String(e), 'error');
        });
    },

    restore(c) {
      if (this.tree && this.tree.dirty) {
        window.alert('You have unapplied changes. Activate or discard them before restoring an earlier version.');
        return;
      }
      var msg =
        'Restore the settings from ' + this.fullTime(c.timestamp) + ' (version ' + c.short + ')?\n\n' +
        'Your settings are switched back to this version and the server is rebuilt and activated. ' +
        'This takes a few minutes and saves a new version on a new branch; nothing is deleted.';
      if (!window.confirm(msg)) return;
      this.post('/versioning/activate/' + encodeURIComponent(c.id), 'Restoring version ' + c.short);
    },

    activateCurrent() {
      var msg =
        'Build and activate your current settings?\n\n' +
        'This runs a full activation (write-flake + nixos-rebuild) and can take several minutes.';
      if (!window.confirm(msg)) return;
      this.post('/actions/activate', 'Activating current settings');
    },

    commitNote(c) {
      return c ? VT.commitNote(this.tree && this.tree.notes, c.id) : '';
    },

    genNoteText(n) {
      return VT.genNote(this.tree && this.tree.notes, n);
    },

    /** Set / clear the note on a version (`commit`, full id) or a generation (number). */
    editNote(kind, id, label) {
      var cur = kind === 'commit' ? VT.commitNote(this.tree && this.tree.notes, id) : this.genNoteText(id);
      var note = window.prompt('Note for ' + label + ' (empty to remove):', cur || '');
      if (note === null) return;
      var self = this;
      fetch('/versioning/notes', {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: VT.noteBody(kind, id, note),
      })
        .then(function (r) {
          return r.json();
        })
        .then(function (res) {
          if (res.error) throw new Error(res.error);
          return self.loadTree();
        })
        .catch(function (e) {
          toast('Saving the note failed: ' + (e && e.message ? e.message : e), 'error');
        });
    },

    switchGen(n) {
      var msg =
        'Switch the running system to generation ' + n + '?\n\n' +
        'Your settings are not changed. The switch runs in the background and may restart ' +
        'this web UI — wait a moment and reload if the page disconnects.';
      if (!window.confirm(msg)) return;
      var self = this;
      this.post('/versioning/generations/' + n + '/switch', 'Switching to generation ' + n).then(function () {
        toast('Generation switch started', 'info');
        setTimeout(function () {
          if (self.$root.isConnected) self.loadTree();
        }, 15000);
      });
    },

    bootGen(n) {
      var msg =
        'Boot into generation ' + n + '?\n\n' +
        'Generation ' + n + ' becomes the boot default and the server restarts right away. ' +
        'All services are down until it is back. Your settings are not changed.';
      if (!window.confirm(msg)) return;
      this.post('/versioning/generations/' + n + '/boot', 'Booting into generation ' + n);
    },
  };
};
