// resources_panel.js — system resources widget on the services overview
// (compact CPU/RAM/disk strip that expands into a btop-inspired detail view:
// per-core CPU, memory incl. ZFS ARC as its own segment, swap, disks/pools,
// network throughput, top processes).
//
// Pure formatting/sorting helpers (NeoResources) are unit tested directly
// (resources_panel.test.js, `just test-widgets`); resourcesPanel() is the
// Alpine component (partials/resources_panel.html.hbs) that polls
// GET /resources/snapshot and wires those helpers into the DOM.
//
// Polling: ~2s while the detail view is expanded, ~5s for the compact strip
// alone, paused entirely while document.hidden or once the element leaves
// the DOM (mirrors service_status.js / versioning.js's poll patterns).
(function (root, factory) {
  var api = factory((root && root.NeoUtil) || require('./neo_util.js'));
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root) {
    root.NeoResources = api;
    root.resourcesPanel = api.panel;
  }
})(typeof window !== 'undefined' ? window : null, function (NeoUtil) {
  'use strict';

  var UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];

  /** e.g. 1536 -> "1.5 KB". Never negative, never throws on non-numbers. */
  function formatBytes(n) {
    n = Number(n);
    if (!isFinite(n) || n < 0) n = 0;
    var i = 0;
    while (n >= 1024 && i < UNITS.length - 1) {
      n /= 1024;
      i++;
    }
    var digits = i === 0 ? 0 : n < 10 ? 1 : 0;
    // Drop a trailing ".0" (e.g. "3.0 MB" -> "3 MB") without losing real
    // fractional precision (e.g. "1.5 KB" stays as-is).
    var out = n.toFixed(digits).replace(/\.0$/, '');
    return out + ' ' + UNITS[i];
  }

  /** Bytes/sec -> "1.2 MB/s". */
  function formatRate(bytesPerSec) {
    return formatBytes(bytesPerSec) + '/s';
  }

  /** Whole seconds -> "3d 4h" / "4h 12m" / "12m" (coarsest two units). */
  function formatUptime(totalSeconds) {
    var s = Math.max(0, Math.floor(Number(totalSeconds) || 0));
    var d = Math.floor(s / 86400);
    s -= d * 86400;
    var h = Math.floor(s / 3600);
    s -= h * 3600;
    var m = Math.floor(s / 60);
    if (d > 0) return d + 'd ' + h + 'h';
    if (h > 0) return h + 'h ' + m + 'm';
    return m + 'm';
  }

  /** Usage percent -> daisyUI tone suffix ('success' | 'warning' | 'error'). */
  function pctTone(pct) {
    pct = Number(pct) || 0;
    if (pct >= 90) return 'error';
    if (pct >= 70) return 'warning';
    return 'success';
  }

  /** New array (never mutates `procs`) sorted by `key`, 'asc' or 'desc'. */
  function sortProcs(procs, key, dir) {
    var list = (procs || []).slice();
    var mul = dir === 'asc' ? 1 : -1;
    list.sort(function (a, b) {
      var av = a ? a[key] : undefined;
      var bv = b ? b[key] : undefined;
      if (typeof av === 'string' || typeof bv === 'string') {
        return mul * String(av || '').localeCompare(String(bv || ''));
      }
      return mul * ((Number(av) || 0) - (Number(bv) || 0));
    });
    return list;
  }

  /** Percent used, guarding against a zero/missing total. */
  function usedPct(used, total) {
    total = Number(total) || 0;
    if (total <= 0) return 0;
    return ((Number(used) || 0) / total) * 100;
  }

  var POLL_EXPANDED_MS = 2000;
  var POLL_COLLAPSED_MS = 5000;

  function resourcesPanel() {
    return {
      expanded: false,
      loading: true,
      errorMsg: '',
      snap: null,
      procKey: 'cpu_pct',
      procDir: 'desc',
      _timer: null,

      init: function () {
        this.poll();
        this._schedule();
        var self = this;
        this._onVis = function () {
          if (document.visibilityState === 'visible') self._kick();
        };
        document.addEventListener('visibilitychange', this._onVis);
      },

      destroy: function () {
        if (this._timer) clearTimeout(this._timer);
        this._timer = null;
        if (this._onVis) document.removeEventListener('visibilitychange', this._onVis);
      },

      toggle: function () {
        this.expanded = !this.expanded;
        if (this.expanded) this._kick();
      },

      /** Re-armed every tick so leaving the page / hiding the tab stops polling. */
      _schedule: function () {
        var self = this;
        if (this._timer) {
          clearTimeout(this._timer);
          this._timer = null;
        }
        if (!this.$root || !this.$root.isConnected) return;
        if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return;
        var ms = this.expanded ? POLL_EXPANDED_MS : POLL_COLLAPSED_MS;
        this._timer = setTimeout(function () {
          self._timer = null;
          self.poll();
          self._schedule();
        }, ms);
      },

      /** Fetch now and restart the poll clock (expand, or tab becomes visible). */
      _kick: function () {
        this.poll();
        this._schedule();
      },

      poll: function () {
        var self = this;
        return fetch('/resources/snapshot', {
          headers: { Accept: 'application/json' },
          credentials: 'same-origin',
        })
          .then(function (r) {
            return NeoUtil.readJson(r);
          })
          .then(function (data) {
            self.snap = data;
            self.loading = false;
            self.errorMsg = '';
          })
          .catch(function () {
            self.loading = false;
            self.errorMsg = 'Resource data unavailable';
          });
      },

      // ---- view helpers (template calls these; kept here so the .hbs stays declarative) ----

      cpuPct: function () {
        return this.snap ? this.snap.cpu.overall_pct : 0;
      },
      memPct: function () {
        if (!this.snap) return 0;
        var m = this.snap.mem;
        return usedPct(m.used_bytes + (m.arc_bytes || 0), m.total_bytes);
      },
      rootDisk: function () {
        if (!this.snap || !this.snap.disks || !this.snap.disks.length) return null;
        for (var i = 0; i < this.snap.disks.length; i++) {
          if (this.snap.disks[i].mount === '/') return this.snap.disks[i];
        }
        return this.snap.disks[0];
      },
      diskPct: function (d) {
        return d ? usedPct(d.used_bytes, d.total_bytes) : 0;
      },
      rootDiskPct: function () {
        return this.diskPct(this.rootDisk());
      },
      /** Share (%) of total memory for one segment of the used/cache/arc/free bar. */
      memShare: function (bytes) {
        if (!this.snap) return 0;
        return usedPct(bytes || 0, this.snap.mem.total_bytes);
      },
      tone: pctTone,
      sortedProcs: function () {
        if (!this.snap) return [];
        return sortProcs(this.snap.procs, this.procKey, this.procDir).slice(0, 15);
      },
      setSort: function (key) {
        if (this.procKey === key) {
          this.procDir = this.procDir === 'desc' ? 'asc' : 'desc';
        } else {
          this.procKey = key;
          this.procDir = key === 'name' ? 'asc' : 'desc';
        }
      },
      loadText: function () {
        if (!this.snap) return '—';
        return this.snap.load.map(function (v) { return v.toFixed(2); }).join(' · ');
      },
      fmtBytes: formatBytes,
      fmtRate: formatRate,
      fmtUptime: formatUptime,
    };
  }

  return {
    formatBytes: formatBytes,
    formatRate: formatRate,
    formatUptime: formatUptime,
    pctTone: pctTone,
    sortProcs: sortProcs,
    usedPct: usedPct,
    panel: resourcesPanel,
  };
});
