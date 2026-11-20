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
      init() {
        var saved = session ? session.getJson(STORE_KEY, null) : null;
        // Drop a filter whose chip is gone.
        if (saved && this.$root.querySelector('[data-neo-filter="' + saved + '"]')) this.filter = saved;
        var self = this;
        this.$nextTick(function () {
          applyFilter(self.$root, self.filter);
          if (root.NeoAccessMap) root.NeoAccessMap.mount(self.$root.querySelector('[data-neo-map]'));
        });
        this.$watch('filter', function (f) {
          if (session) session.setJson(STORE_KEY, f);
          applyFilter(self.$root, f);
        });
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

  var NeoAccessFilter = { tagsMatch: tagsMatch, applyFilter: applyFilter };
  root.NeoAccessFilter = NeoAccessFilter;
  root.accessFilter = accessFilter;
  if (typeof module === 'object' && module.exports) {
    module.exports = NeoAccessFilter;
  }
})(typeof globalThis !== 'undefined' ? globalThis : this);
