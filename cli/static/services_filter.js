// services_filter.js
// Pure matching predicate for the Services tab (search + status/category/plugin
// source filters). No DOM access, so it is shared as-is between the live UI
// (configuration.js: servicesGrid()) and its Node unit tests via require().
//
// A "card" is the plain-data shape read off a `[data-svc]` element's dataset;
// `cardFromDataset` adapts a DOMStringMap (or any {enabled,cat,pluginIds,search}
// object) into it.
(function (root) {
  'use strict';

  /**
   * @param {{enabled: boolean, cat: string, pluginIds: string[], search: string}} card
   * @param {{q?: string, status?: string, category?: string, plugin?: string}} filters
   * @returns {boolean}
   */
  function serviceMatches(card, filters) {
    filters = filters || {};
    var status = filters.status || 'all';
    var category = filters.category || 'all';
    var plugin = filters.plugin || 'all';
    var q = (filters.q || '').trim().toLowerCase();

    if (status === 'installed' && !card.enabled) return false;
    if (status === 'available' && card.enabled) return false;
    if (category !== 'all' && card.cat !== category) return false;
    if (plugin !== 'all') {
      var urls = card.pluginIds || [];
      if (plugin === 'core' ? urls.length !== 0 : urls.indexOf(plugin) === -1) return false;
    }
    if (q) {
      var hay = (card.search || '').toLowerCase();
      var terms = q.split(/\s+/);
      for (var i = 0; i < terms.length; i++) {
        if (hay.indexOf(terms[i]) === -1) return false;
      }
    }
    return true;
  }

  /** Adapt a `[data-svc]` element's `dataset` (or a plain lookalike) into a card. */
  function cardFromDataset(dataset) {
    dataset = dataset || {};
    return {
      enabled: dataset.enabled === 'true',
      cat: dataset.cat,
      pluginIds: (dataset.pluginIds || '').split('|').filter(Boolean),
      search: dataset.search || '',
    };
  }

  var NeoServicesFilter = {
    serviceMatches: serviceMatches,
    cardFromDataset: cardFromDataset,
  };

  root.NeoServicesFilter = NeoServicesFilter;
  if (typeof module === 'object' && module.exports) {
    module.exports = NeoServicesFilter;
  }
})(typeof globalThis !== 'undefined' ? globalThis : this);
