// access_filter.js
// Quick filters of the Access page (access.html.hbs, x-data="accessFilter()").
// Rows carry their tags in data-neo-tags (computed in routes/access.rs and
// routes/access_ports.rs). Apps that do not match are hidden; the port diagram
// dims them instead, so its lines keep their places. The choice survives the
// re-render after every toggle (sessionStorage).
(function (root) {
  'use strict';

  /** True when the space-separated `tags` contain `filter` ("all" matches all). */
  function tagsMatch(tags, filter) {
    if (!filter || filter === 'all') return true;
    return String(tags || '')
      .split(/\s+/)
      .indexOf(filter) !== -1;
  }

  /** True when `from` (space-separated place ids) contains `origin` (none: all). */
  function reachedFrom(from, origin) {
    if (!origin) return true;
    return String(from || '')
      .split(/\s+/)
      .indexOf(origin) !== -1;
  }

  /** Show, hide or dim every tagged element under `el`; returns matches per section. */
  function applyFilter(el, filter) {
    var found = {};
    el.querySelectorAll('[data-neo-tags]').forEach(function (row) {
      var ok = tagsMatch(row.getAttribute('data-neo-tags'), filter);
      var sec = row.getAttribute('data-neo-section') || '';
      if (ok && sec) found[sec] = (found[sec] || 0) + 1;
      if (row.getAttribute('data-neo-filter-mode') === 'dim') {
        row.classList.toggle('opacity-15', !ok);
      } else {
        row.hidden = !ok;
      }
    });
    el.querySelectorAll('[data-neo-filter-empty]').forEach(function (note) {
      note.hidden = filter === 'all' || (found[note.getAttribute('data-neo-filter-empty')] || 0) > 0;
    });
    return found;
  }

  var STORE_KEY = 'neo.accessFilter';

  /** Alpine component on #access-page. */
  function accessFilter() {
    var session = root.NeoUtil ? root.NeoUtil.store('session') : null;
    return {
      filter: 'all',
      // Place selected in the map (a phone lists only what it reaches).
      origin: null,
      init() {
        var phone = false;
        try {
          phone = !!root.matchMedia && !root.matchMedia('(min-width: 1024px)').matches;
        } catch (e) {}
        var savedPin = session ? session.getJson('neo.accessPin', undefined) : undefined;
        if (typeof savedPin === 'string' && savedPin.indexOf('o-') === 0) this.origin = savedPin.slice(2);
        // A phone shows one place at a time: start with the internet.
        else if (savedPin === undefined && phone) this.origin = 'web';
        var saved = session ? session.getJson(STORE_KEY, null) : null;
        // Drop a filter whose chip is gone.
        if (saved && this.$root.querySelector('[data-neo-filter="' + saved + '"]')) this.filter = saved;
        var self = this;
        // Before the first paint, so the rows do not show and then vanish.
        applyFilter(self.$root, self.filter);
        this.$nextTick(function () {
          if (root.NeoAccessMap) {
            root.NeoAccessMap.mount(self.$root.querySelector('[data-neo-map]'), phone ? 'o-web' : null);
          }
        });
        this.$watch('filter', function (f) {
          if (session) session.setJson(STORE_KEY, f);
          applyFilter(self.$root, f);
        });
      },
      /** The map selected a box: a place also picks what a phone lists. */
      onPin(d) {
        if (d && d.id && d.id.indexOf('o-') === 0) this.origin = d.id.slice(2);
        else if (d && !d.id && d.prev && d.prev.indexOf('o-') === 0) this.origin = null;
      },
      reaches(el) {
        return reachedFrom(el.getAttribute('data-neo-from'), this.origin);
      },
      pick(f) {
        this.filter = this.filter === f ? 'all' : f;
      },
      /** From the map or the verdict: filter the lists and scroll to them. */
      show(f) {
        this.filter = f;
        var lists = this.$root.querySelector('#access-lists');
        if (lists) lists.scrollIntoView({ behavior: 'smooth', block: 'start' });
      },
    };
  }

  // A change re-renders #access-page: keep the page where it was.
  if (typeof document !== 'undefined' && document.addEventListener) {
    var kept = null;
    document.addEventListener('htmx:beforeSwap', function (e) {
      var t = e.detail && e.detail.target;
      kept = t && t.id === 'access-page' ? root.scrollY : null;
    });
    document.addEventListener('htmx:afterSettle', function (e) {
      var t = e.detail && e.detail.target;
      if (kept !== null && t && (t.id === 'access-page' || !t.isConnected)) {
        root.scrollTo(0, kept);
        kept = null;
      }
    });
  }

  var NeoAccessFilter = { tagsMatch: tagsMatch, applyFilter: applyFilter, reachedFrom: reachedFrom };
  root.NeoAccessFilter = NeoAccessFilter;
  root.accessFilter = accessFilter;
  if (typeof module === 'object' && module.exports) {
    module.exports = NeoAccessFilter;
  }
})(typeof globalThis !== 'undefined' ? globalThis : this);
