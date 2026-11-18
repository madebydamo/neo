// configuration.js
// The config page shell: Alpine tabs + breadcrumb, the unsaved-changes guard, the sticky
// bars, and syncing them with what htmx swaps in. Loaded only by configuration.html.hbs,
// after configuration_core.js (NeoConfigCore: the decisions without DOM access).

NeoUtil.markStandalone();

/**
 * True when #options-pane exists and any option field differs from its original value.
 * Uses Alpine.$data(pane) (optionForm: values / originals / isAtOriginal).
 */
window.neoIsConfigFormDirty = function neoIsConfigFormDirty() {
  try {
    var pane = document.getElementById('options-pane');
    if (!pane) return false;
    if (typeof Alpine === 'undefined' || !Alpine.$data) return false;
    return NeoConfigCore.formDirty(Alpine.$data(pane));
  } catch (e) {
    return false;
  }
};

/**
 * If the option form is dirty, ask the user (in-page dialog) to discard unsaved changes.
 * Resolves true when navigation may proceed (not dirty, or user confirmed discard).
 */
window.neoConfirmLeaveConfigForm = function neoConfirmLeaveConfigForm() {
  if (!window.neoIsConfigFormDirty()) return Promise.resolve(true);
  return NeoUtil.confirm('You have unsaved changes. Discard them?', { okLabel: 'Discard', danger: true });
};

// One-shot: loadTab/goUp confirm first, then set this so htmx:beforeRequest does not re-prompt.
window.neoAllowNextConfigNav = false;

/** Alpine shell for the config page: tabs, detail breadcrumb, and up navigation. */
window.configShell = function configShell() {
  var el = document.getElementById('config-shell');
  var initialTab = (el && el.getAttribute('data-initial-tab')) || 'services';
  var initialDetail = el && el.getAttribute('data-initial-detail');
  if (initialDetail === '') initialDetail = null;
  return {
    tab: initialTab, // 'services' | 'guides' | 'settings' | 'versioning'
    detail: initialDetail || null, // null | service/core section name when option pane is open

    sectionLabel() {
      return NeoConfigCore.sectionLabel(this.tab);
    },

    setDetail(name) {
      this.detail = name || null;
    },

    clearDetail() {
      this.detail = null;
    },

    /** Canonical tab content URL under /configuration/... */
    tabUrl(tab) {
      return NeoConfigCore.tabUrl(tab);
    },

    /**
     * Navigate #config-content and push the URL into history.
     *
     * htmx 1.9 `htmx.ajax(..., { push })` is ignored — history only comes from
     * `hx-push-url` on the *source* element. Pass #config-nav-helper as source
     * (it has hx-push-url="true") so the request path is pushed like card nav.
     */
    navigateContent(url) {
      if (typeof htmx === 'undefined' || !htmx.ajax) return;
      window.neoAllowNextConfigNav = true;
      var helper = document.getElementById('config-nav-helper');
      var opts = {
        target: '#config-content',
        swap: 'innerHTML',
      };
      if (helper) {
        opts.source = helper;
      }
      var p = htmx.ajax('GET', url, opts);
      // If helper is missing, still update the location bar after the swap.
      if (!helper && p && typeof p.then === 'function') {
        p.then(function () {
          try {
            if (location.pathname + location.search !== url) {
              history.pushState({ htmx: true }, '', url);
            }
          } catch (e) {}
        });
      }
    },

    /**
     * Switch tab and load the corresponding grid/branches into #config-content.
     * Do not clear `detail` here — breadcrumb chrome stays until afterSwap so the
     * layout does not collapse while the option pane is still on screen.
     */
    loadTab(tab) {
      var self = this;
      window.neoConfirmLeaveConfigForm().then(function (ok) {
        if (!ok) return;
        self.tab = tab;
        self.navigateContent(self.tabUrl(tab));
      });
    },

    /**
     * Leave the option pane and return to the parent grid for the current tab.
     * Breadcrumb stays visible until the grid swap lands (see syncConfigShellFromContent).
     */
    goUp() {
      var self = this;
      window.neoConfirmLeaveConfigForm().then(function (ok) {
        if (ok) self.navigateContent(self.tabUrl(self.tab));
      });
    },

    /** Logo / home: services tab at /configuration. */
    goHome() {
      var self = this;
      window.neoConfirmLeaveConfigForm().then(function (ok) {
        if (!ok) return;
        self.tab = 'services';
        self.navigateContent('/configuration');
      });
    },
  };
};

(function () {
  /** Client-side page load in flight (HTMX → #config-content). */
  var navLoadBusy = false;

  /** Server-driven nix eval busy, mirrored from #action-bar-dynamic[data-eval-busy]. */
  function isEvalBusy() {
    var bar = document.getElementById('action-bar-dynamic');
    return !!(bar && bar.getAttribute('data-eval-busy') === 'true');
  }

  /** Single navbar ring for page load and/or nix eval (plain "Loading" label). */
  function syncBarBusy() {
    var el = document.getElementById('nav-busy');
    if (!el) return;
    var on = navLoadBusy || isEvalBusy();
    el.classList.toggle('is-busy', on);
    el.setAttribute('aria-hidden', on ? 'false' : 'true');
  }

  /** Instant navbar spinner for config navigations (no CSS fade); driven by nav_progress.js. */
  window.neoSetNavBusy = function (on) {
    navLoadBusy = !!on;
    syncBarBusy();
  };

  /** After action-bar swaps (HTTP or WS OOB), re-fold data-eval-busy into #nav-busy. */
  function syncBarBusyIfActionBar(evt) {
    try {
      var t = (evt.detail || {}).target;
      if (!t) return;
      if (t.id === 'action-bar-dynamic') {
        syncBarBusy();
        return;
      }
      if (t.querySelector && t.querySelector('#action-bar-dynamic')) {
        syncBarBusy();
      }
    } catch (e) {}
  }

  /**
   * Hold #config-content's current height while a nav request is in flight so the
   * document does not collapse when a tall option pane is swapped for a shorter grid
   * (or while waiting for the response). Cleared after the swap settles.
   */
  function lockConfigContentHeight() {
    var el = document.getElementById('config-content');
    if (!el) return;
    var h = el.offsetHeight;
    if (h > 0) el.style.minHeight = h + 'px';
  }

  function unlockConfigContentHeight() {
    var el = document.getElementById('config-content');
    if (!el) return;
    el.style.minHeight = '';
  }

  // Guard attribute-based (and other) HTMX navigations that replace #config-content
  // (NeoUtil runs request guards on htmx:confirm, before hx-confirm's question).
  // Skip modals, action bar, unit controls, changes-body, etc. (other targets).
  // After a successful save, option_form updates originals before reload, so dirty is false.
  NeoUtil.guardRequests(function (detail) {
    try {
      if (!NeoConfigCore.isNavRequest(detail)) return true;
      if (window.neoAllowNextConfigNav) {
        window.neoAllowNextConfigNav = false;
        return true;
      }
      if (!window.neoIsConfigFormDirty()) return true;
      return window.neoConfirmLeaveConfigForm();
    } catch (e) {
      return true;
    }
  });

  document.body.addEventListener('htmx:beforeRequest', function (evt) {
    try {
      if (NeoConfigCore.isNavRequest(evt.detail || {})) lockConfigContentHeight();
    } catch (e) {}
  });

  document.body.addEventListener('htmx:afterSwap', syncBarBusyIfActionBar);
  document.body.addEventListener('htmx:oobAfterSwap', syncBarBusyIfActionBar);

  // A WS OOB push can replace an element (e.g. #action-bar-dynamic) while its own
  // hx-get is in flight; the late response would then target a detached node and
  // throw inside htmx. Abort that request while the element is still attached.
  document.body.addEventListener('htmx:oobBeforeSwap', function (evt) {
    var t = evt.detail && evt.detail.target;
    if (t && t.classList && t.classList.contains('htmx-request')) htmx.trigger(t, 'htmx:abort');
  });

  // WS/OOB replaces #action-bar-dynamic; watch its parent so data-eval-busy
  // still folds into #nav-busy when htmx target events are sparse.
  (function observeEvalBusy() {
    if (typeof MutationObserver === 'undefined') return;
    var bar = document.getElementById('action-bar-dynamic');
    var parent = bar && bar.parentNode;
    if (!parent) return;
    var mo = new MutationObserver(function () {
      syncBarBusy();
    });
    mo.observe(parent, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ['data-eval-busy'],
    });
  })();
  ['htmx:responseError', 'htmx:sendError'].forEach(function (name) {
    document.body.addEventListener(name, function (evt) {
      try {
        if (NeoConfigCore.isConfigContentTarget((evt.detail || {}).target)) unlockConfigContentHeight();
      } catch (e) {
        unlockConfigContentHeight();
      }
    });
  });

  // Browser leave / hard navigation while the option form has unsaved edits.
  window.addEventListener('beforeunload', function (e) {
    try {
      if (!window.neoIsConfigFormDirty()) return;
      e.preventDefault();
      e.returnValue = '';
    } catch (err) {}
  });

  /**
   * `?focus=<option>` (guide option: links): open the option's section, scroll
   * to the field and flash it once.
   */
  function focusOptionFromUrl(pane) {
    var name = null;
    try {
      name = new URLSearchParams(location.search).get('focus');
    } catch (e) {}
    if (!name) return;
    var field = null;
    pane.querySelectorAll('[data-option]').forEach(function (el) {
      if (!field && el.getAttribute('data-option') === name) field = el;
    });
    if (!field) return;
    var section = field.closest('section[data-section]');
    try {
      if (section && typeof Alpine !== 'undefined' && Alpine.$data) {
        var d = Alpine.$data(field);
        if (d && d.openSections) {
          var next = Object.assign({}, d.openSections);
          next[section.getAttribute('data-section')] = true;
          d.openSections = next;
        }
        if (d && d.setPaneTab) d.setPaneTab('settings');
      }
    } catch (e) {}
    setTimeout(function () {
      field.scrollIntoView({ block: 'center', behavior: 'smooth' });
      field.classList.add('neo-guide-focus');
      setTimeout(function () {
        field.classList.remove('neo-guide-focus');
      }, 2400);
    }, 150);
  }

  document.body.addEventListener('htmx:afterRequest', function (evt) {
    var d = evt.detail || {};
    var path = (d.pathInfo && d.pathInfo.requestPath) || '';
    // After discarding changes the open pane would hold stale values.
    if (d.successful && NeoConfigCore.discardsChanges(path)) NeoUtil.reloadPane();
  });

  // Deep links with ?focus= (guide option: links) on a full page load.
  NeoUtil.onReady(document, function () {
    setTimeout(function () {
      var pane = document.getElementById('options-pane');
      if (pane) focusOptionFromUrl(pane);
    }, 400);
  });

  /** Sync shell.detail / shell.tab after HTMX swaps into #config-content. */
  function syncConfigShellFromContent() {
    try {
      var shell = document.getElementById('config-shell');
      if (!shell || typeof Alpine === 'undefined' || !Alpine.$data) return;
      var data = Alpine.$data(shell);
      if (!data) return;
      var s = NeoConfigCore.shellState(document.getElementById('config-content'));
      if (s.detail) data.setDetail(s.detail);
      else data.clearDetail();
      if (s.tab) data.tab = s.tab;
      if (s.pane) focusOptionFromUrl(s.pane);
    } catch (e) {}
  }

  /** Scroll the config page (and parent iframe, if embedded) back to the top. */
  function resetConfigViewport() {
    try {
      window.scrollTo(0, 0);
      if (document.documentElement) document.documentElement.scrollTop = 0;
      if (document.body) document.body.scrollTop = 0;
      // When neo config is shown inside the navigator iframe, also reset the outer frame.
      try {
        if (window.parent && window.parent !== window) {
          window.parent.scrollTo(0, 0);
        }
      } catch (e) {}
    } catch (e) {}
  }

  /** Hide the shared fused glass plate and clear fuse state. */
  function clearStickyBarsFuse() {
    try {
      document.documentElement.classList.remove('neo-bars-fused');
      var glass = document.getElementById('config-fused-glass');
      if (glass) {
        glass.hidden = true;
        glass.removeAttribute('style');
      }
    } catch (e) {}
  }

  /**
   * Fuse the sticky action bar + option-pane save strip into one glass chrome
   * when the save strip is stuck under the navbar. A single fixed plate carries
   * backdrop-filter (two layered blurs cannot merge at the seam). Cleared when
   * the pane is gone or the strip is still in normal document flow.
   */
  function updateStickyBarsFuse() {
    try {
      var save = document.getElementById('options-sticky-save');
      var action = document.getElementById('config-action-bar');
      var glass = document.getElementById('config-fused-glass');
      var root = document.documentElement;
      if (!save || !action) {
        clearStickyBarsFuse();
        return;
      }
      var stickyTop = parseFloat(getComputedStyle(save).top) || 0;
      var plate = NeoConfigCore.fusedPlate(stickyTop, action.getBoundingClientRect(), save.getBoundingClientRect());
      if (!plate) {
        clearStickyBarsFuse();
        return;
      }
      root.classList.add('neo-bars-fused');
      if (!glass) return;
      // One plate from the top of the action bar to the bottom of the save strip,
      // matching the action bar's width so the fused chrome is continuous.
      glass.hidden = false;
      glass.style.top = plate.top + 'px';
      glass.style.left = plate.left + 'px';
      glass.style.width = plate.width + 'px';
      glass.style.height = plate.height + 'px';
    } catch (e) {
      clearStickyBarsFuse();
    }
  }

  var stickyFuseRaf = 0;
  function scheduleStickyBarsFuse() {
    if (stickyFuseRaf) return;
    stickyFuseRaf = requestAnimationFrame(function () {
      stickyFuseRaf = 0;
      updateStickyBarsFuse();
    });
  }

  window.addEventListener('scroll', scheduleStickyBarsFuse, { passive: true });
  window.addEventListener('resize', scheduleStickyBarsFuse);

  // Sync chrome as soon as content lands (afterSwap), not afterSettle — so the
  // breadcrumb is updated in the same paint cycle as the pane/grid swap.
  // Then release the height lock and scroll on the next frame so Alpine can
  // apply detail chrome before the document reflows (one layout change, not two).
  document.body.addEventListener('htmx:afterSwap', function (evt) {
    try {
      var t = evt.detail && evt.detail.target;
      if (!t) return;
      if (NeoUtil.isContentSwap(t)) {
        syncConfigShellFromContent();
        requestAnimationFrame(function () {
          unlockConfigContentHeight();
          // Opening/closing the option pane (or switching tabs) replaces the main
          // content while preserving document scroll — always start at the top.
          resetConfigViewport();
          updateStickyBarsFuse();
        });
      }
    } catch (e) {}
  });

  // htmx's Back/Forward restore swaps #config-content straight from its history
  // cache (bypassing the ajax pipeline), so it never fires htmx:afterSwap — the
  // tab highlight / breadcrumb would otherwise keep whatever they were before
  // navigating away, out of sync with the just-restored content. Re-derive them
  // from the restored DOM the same way afterSwap does.
  document.body.addEventListener('htmx:historyRestore', function () {
    syncConfigShellFromContent();
  });

  // Only on full page load: reopen the op monitor that was running before a reload.
  NeoUtil.onReady(document, function () {
    if (window.NeoOpMonitor) window.NeoOpMonitor.resume(window);
    updateStickyBarsFuse();
  });

  /**
   * Empty the shared dialog on close. Op monitors (static/op_monitor.js) close
   * their WebSocket once their element leaves the DOM.
   */
  function clearChangesMonitor() {
    var body = document.getElementById('changes-body');
    if (!body) return;
    body.innerHTML = '';
  }

  var changesModal = document.getElementById('changes-modal');
  if (changesModal) {
    changesModal.addEventListener('close', clearChangesMonitor);
  }

  // Suppress noisy htmx swapErrors for OOB unit-controls updates when the row
  // is no longer in the DOM (e.g. switched panes, closed dialog, or rapid actions).
  // The status will be correct again on next pane load (which does a fresh bootstrap GET).
  document.body.addEventListener('htmx:swapError', function (evt) {
    try {
      var resp = evt.detail && evt.detail.xhr && evt.detail.xhr.responseText;
      if (NeoConfigCore.isStaleUnitSwap(resp)) {
        evt.stopImmediatePropagation();
      }
    } catch (e) {}
  });
})();
