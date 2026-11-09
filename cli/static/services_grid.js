// services_grid.js
// The Services tab's Alpine component (x-data="servicesGrid()"); matching rules live in
// services_filter.js.

/**
 * Alpine state for the Services tab: search + status / category / source filters.
 * Cards (`[data-svc]`) carry data-enabled / data-cat / data-plugin-ids / data-search.
 * Filters persist per browser tab (sessionStorage) so returning from a service keeps them.
 * Matching itself lives in services_filter.js (NeoServicesFilter), shared with its
 * Node unit tests, so the tab count badges reuse the exact same rules as the cards.
 */
window.servicesGrid = function servicesGrid() {
  var STORE_KEY = 'neo.servicesFilters';
  var session = window.NeoUtil.store('session');
  return {
    q: '',
    status: 'all', // all | installed | available
    category: 'all',
    plugin: 'all', // all | core | <plugin id>
    counts: { all: 0, installed: 0, available: 0 },

    init() {
      var cards = this.$root.querySelectorAll('[data-svc]');
      var installed = 0;
      cards.forEach(function (c) {
        if (c.dataset.enabled === 'true') installed++;
      });
      this.counts = { all: cards.length, installed: installed, available: cards.length - installed };
      var saved = session.getJson(STORE_KEY, null);
      if (saved) {
        this.q = saved.q || '';
        this.status = saved.status || 'all';
        this.category = saved.category || 'all';
        this.plugin = saved.plugin || 'all';
      }
      // Drop stale filters (category / plugin no longer present after a config change).
      var root = this.$root;
      var hasCard = function (pred) {
        return Array.prototype.some.call(cards, pred);
      };
      var cat = this.category;
      if (cat !== 'all' && !hasCard(function (c) { return c.dataset.cat === cat; })) this.category = 'all';
      var pl = this.plugin;
      if (pl !== 'all' && !root.querySelector('select option[value="' + (window.CSS && CSS.escape ? CSS.escape(pl) : pl) + '"]')) {
        this.plugin = 'all';
      }
      var self = this;
      this.$watch('q', function () { self.persist(); });
      this.$watch('status', function () { self.persist(); });
      this.$watch('category', function () { self.persist(); });
      this.$watch('plugin', function () { self.persist(); });
    },

    persist() {
      session.setJson(STORE_KEY, { q: this.q, status: this.status, category: this.category, plugin: this.plugin });
    },

    /** "/" focuses search (unless typing in another field). */
    onKey(e) {
      if (e.key !== '/' || e.ctrlKey || e.metaKey || e.altKey) return;
      var t = e.target;
      var tag = (t && t.tagName) || '';
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || (t && t.isContentEditable)) return;
      if (document.querySelector('dialog[open]')) return;
      e.preventDefault();
      if (this.$refs.search) this.$refs.search.focus();
    },

    cardVisible(el) {
      return this.cardMatches(el, null);
    },

    /**
     * Match `el` against the current filters, with any of status/category/plugin
     * replaced by `overrides`. Delegates to NeoServicesFilter (services_filter.js)
     * so cards, category/source availability and the tab count badges all agree.
     */
    matches(el, overrides) {
      overrides = overrides || {};
      // Read every filter up front so Alpine tracks all of them as dependencies,
      // even the ones an override below discards.
      var filters = {
        q: this.q,
        status: 'status' in overrides ? overrides.status : this.status,
        category: 'category' in overrides ? overrides.category : this.category,
        plugin: 'plugin' in overrides ? overrides.plugin : this.plugin,
      };
      return window.NeoServicesFilter.serviceMatches(
        window.NeoServicesFilter.cardFromDataset(el.dataset),
        filters
      );
    },

    /** Match against every filter except `skip` ('category' | 'plugin'). */
    cardMatches(el, skip) {
      if (skip === 'category') return this.matches(el, { category: 'all' });
      if (skip === 'plugin') return this.matches(el, { plugin: 'all' });
      return this.matches(el, {});
    },

    /** Cards that would be visible on tab `status`, given the other active filters. */
    tabCount(status) {
      var self = this;
      var cards = this.$root.querySelectorAll('[data-svc]');
      var n = 0;
      for (var i = 0; i < cards.length; i++) {
        if (self.matches(cards[i], { status: status })) n++;
      }
      return n;
    },

    /** Category chip is offered when some card in it matches the other filters. */
    categoryAvailable(cat) {
      if (cat === this.category) return true;
      var self = this;
      return Array.prototype.some.call(this.$root.querySelectorAll('[data-svc]'), function (c) {
        return c.dataset.cat === cat && self.cardMatches(c, 'category');
      });
    },

    /** Source option is offered when some card from it matches the other filters. */
    pluginAvailable(url) {
      if (url === this.plugin) return true;
      var self = this;
      return Array.prototype.some.call(this.$root.querySelectorAll('[data-svc]'), function (c) {
        var urls = (c.dataset.pluginIds || '').split('|').filter(Boolean);
        var from = url === 'core' ? urls.length === 0 : urls.indexOf(url) !== -1;
        return from && self.cardMatches(c, 'plugin');
      });
    },

    visibleCount(section) {
      if (!section) return 0;
      var self = this;
      return Array.prototype.filter.call(section.querySelectorAll('[data-svc]'), function (c) {
        return self.cardVisible(c);
      }).length;
    },

    sectionVisible(section) {
      return this.visibleCount(section) > 0;
    },

    anyVisible() {
      return this.visibleCount(this.$root) > 0;
    },

    resetFilters() {
      this.q = '';
      this.status = 'all';
      this.category = 'all';
      this.plugin = 'all';
    },
  };
};
