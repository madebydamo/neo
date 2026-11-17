// configuration_core.js — the config page shell's decisions without DOM access
// (configuration.js, loaded after this file): tab labels and URLs, which htmx
// requests navigate #config-content, the unsaved-changes check, which tab and
// breadcrumb a swapped-in page means, and where the fused sticky-bar plate
// goes. Tested by configuration_core.test.js.
(function (root, factory) {
  var api = factory();
  if (root) root.NeoConfigCore = api;
  if (typeof module === 'object' && module.exports) module.exports = api;
})(typeof window !== 'undefined' ? window : null, function () {
  'use strict';

  /** Breadcrumb label of a tab ('services' | 'guides' | 'settings' | 'versioning'). */
  function sectionLabel(tab) {
    if (tab === 'settings') return 'Settings';
    if (tab === 'versioning') return 'Versioning';
    if (tab === 'guides') return 'Guides';
    return 'Services';
  }

  /** Canonical tab content URL under /configuration/... */
  function tabUrl(tab) {
    if (tab === 'settings') return '/configuration/settings';
    if (tab === 'versioning') return '/configuration/versioning';
    if (tab === 'guides') return '/configuration/guides';
    // Prefer /configuration (not /configuration/services) as the services home URL.
    return '/configuration';
  }

  /** Whether an htmx target (element or selector string) is #config-content. */
  function isConfigContentTarget(target) {
    if (!target) return false;
    if (typeof target === 'string') {
      return target === '#config-content' || target === 'config-content';
    }
    if (target.id === 'config-content') return true;
    // htmx sometimes reports the requesting elt; treat swaps into #config-content as nav.
    return !!(target.getAttribute && target.getAttribute('hx-target') === '#config-content');
  }

  /** A request that replaces #config-content, however htmx reports its target. */
  function isNavRequest(detail) {
    return (
      isConfigContentTarget(detail.target) ||
      isConfigContentTarget(detail.elt) ||
      isConfigContentTarget(detail.requestConfig && detail.requestConfig.target)
    );
  }

  /** Whether an option form's Alpine data (values / isAtOriginal) holds unsaved edits. */
  function formDirty(data) {
    if (!data || !data.values || typeof data.isAtOriginal !== 'function') return false;
    var names = Object.keys(data.values);
    for (var i = 0; i < names.length; i++) {
      if (!data.isAtOriginal(names[i])) return true;
    }
    return false;
  }

  /** Requests after which the open pane holds stale values (changes discarded). */
  function discardsChanges(path) {
    return path === '/actions/reset' || path === '/changes/revert';
  }

  /**
   * Tab and breadcrumb for what #config-content shows: { pane, detail, tab }.
   * `content` needs querySelector; `detail` null clears the breadcrumb; `tab`
   * null keeps the current one; `pane` is the option pane, if any.
   */
  function shellState(content) {
    var q = function (sel) {
      return content ? content.querySelector(sel) : null;
    };
    var pane = q('#options-pane');
    if (pane) {
      return {
        pane: pane,
        detail: pane.getAttribute('data-service'),
        tab: pane.getAttribute('data-is-core') === 'true' ? 'settings' : 'services',
      };
    }
    var guide = q('#guide-page');
    if (guide) return { pane: null, detail: guide.getAttribute('data-guide-title') || 'Guide', tab: 'guides' };
    if (q('#privacy-map')) return { pane: null, detail: 'Privacy map', tab: 'guides' };
    // Infer the active tab from grid/branches partials (Back/Forward history restore).
    var tab = null;
    if (q('#core-grid')) tab = 'settings';
    else if (q('#versioning-root') || q('#branches-section')) tab = 'versioning';
    else if (q('#guides-grid')) tab = 'guides';
    else if (q('#services-grid')) tab = 'services';
    return { pane: null, detail: null, tab: tab };
  }

  /**
   * The fused glass plate under the action bar and the option-pane save strip:
   * null while the strip is not stuck (`stickyTop` = its CSS top), else the
   * plate box from the top of the action bar to the bottom of the strip.
   */
  function fusedPlate(stickyTop, actionRect, saveRect) {
    if (!(saveRect.top <= stickyTop + 0.5)) return null;
    return {
      top: actionRect.top,
      left: actionRect.left,
      width: actionRect.width,
      height: Math.max(0, saveRect.bottom - actionRect.top),
    };
  }

  /** A swap error from an OOB unit-controls / update-out update whose row is gone. */
  function isStaleUnitSwap(responseText) {
    return !!responseText && (/id="unit-controls-/.test(responseText) || /id="update-out-/.test(responseText));
  }

  return {
    sectionLabel: sectionLabel,
    tabUrl: tabUrl,
    isConfigContentTarget: isConfigContentTarget,
    isNavRequest: isNavRequest,
    formDirty: formDirty,
    discardsChanges: discardsChanges,
    shellState: shellState,
    fusedPlate: fusedPlate,
    isStaleUnitSwap: isStaleUnitSwap,
  };
});
