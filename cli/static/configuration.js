// configuration.js
// Client helpers for the homeserver config page: activation resume, live unit logs (SSE),
// WebSocket unit-watch registration, and the Alpine config shell (tabs + breadcrumb).
// Loaded only by configuration.html.hbs.

// Home-screen launch (iOS sets navigator.standalone; others use display-mode).
(function markStandalone() {
  try {
    var standalone =
      window.navigator.standalone === true ||
      window.matchMedia('(display-mode: standalone)').matches ||
      window.matchMedia('(display-mode: fullscreen)').matches ||
      window.matchMedia('(display-mode: minimal-ui)').matches;
    if (!standalone) return;
    document.documentElement.classList.add('neo-standalone');
    if (document.body) document.body.classList.add('neo-standalone');
  } catch (e) {}
})();

/**
 * Show a transient DaisyUI alert toast outside #config-content.
 * @param {string} message
 * @param {'success'|'error'|'info'|'warning'} [type]
 */
window.neoToast = function neoToast(message, type) {
  var host = document.getElementById('toast-host');
  if (!host) return;
  var kind = type || 'info';
  var alertClass =
    kind === 'success'
      ? 'alert-success'
      : kind === 'error'
        ? 'alert-error'
        : kind === 'warning'
          ? 'alert-warning'
          : 'alert-info';
  var el = document.createElement('div');
  el.className = 'alert ' + alertClass + ' shadow-lg text-sm max-w-sm';
  el.setAttribute('role', 'status');

  var msg = document.createElement('span');
  msg.textContent = String(message == null ? '' : message);
  el.appendChild(msg);

  var dismiss = document.createElement('button');
  dismiss.type = 'button';
  dismiss.className = 'btn btn-ghost btn-xs btn-circle';
  dismiss.setAttribute('aria-label', 'Dismiss');
  dismiss.textContent = '✕';
  dismiss.addEventListener('click', function () {
    if (el.parentNode) el.parentNode.removeChild(el);
  });
  el.appendChild(dismiss);

  host.appendChild(el);
  setTimeout(function () {
    if (el.parentNode) el.parentNode.removeChild(el);
  }, 4000);
};

/**
 * Download working-tree settings.toml without navigating this document.
 * A plain <a href="/settings/download"> would replace the page when this UI
 * is embedded in the navigator iframe (or when the attachment is blocked).
 */
window.neoDownloadSettingsToml = function neoDownloadSettingsToml() {
  var status = document.getElementById('settings-toml-status');
  fetch('/settings/download')
    .then(function (res) {
      if (!res.ok) {
        throw new Error(res.status === 404 ? 'settings.toml not found' : 'Download failed');
      }
      return res.blob();
    })
    .then(function (blob) {
      var url = URL.createObjectURL(blob);
      var a = document.createElement('a');
      a.href = url;
      a.download = 'settings.toml';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      setTimeout(function () {
        URL.revokeObjectURL(url);
      }, 1000);
    })
    .catch(function (err) {
      if (typeof neoToast === 'function') {
        neoToast('Download failed: ' + err, 'error');
      }
      if (status) {
        status.innerHTML = '<div class="alert alert-error text-sm">Download failed</div>';
      }
    });
};

/**
 * Replace working-tree settings.toml from a file input (Settings tab).
 * @param {HTMLInputElement} input
 */
window.neoUploadSettingsToml = function neoUploadSettingsToml(input) {
  var file = input.files && input.files[0];
  input.value = '';
  if (!file) return;
  var ok = window.confirm(
    'Replace working-tree settings.toml with "' +
      file.name +
      '"? Review pending changes and apply to activate.'
  );
  if (!ok) return;
  var status = document.getElementById('settings-toml-status');
  file
    .text()
    .then(function (text) {
      return fetch('/settings/upload', {
        method: 'POST',
        headers: { 'Content-Type': 'text/plain; charset=utf-8' },
        body: text,
      });
    })
    .then(function (res) {
      return res.text();
    })
    .then(function (html) {
      if (status) status.innerHTML = html;
    })
    .catch(function (err) {
      if (typeof neoToast === 'function') {
        neoToast('Upload failed: ' + err, 'error');
      }
      if (status) {
        status.innerHTML = '<div class="alert alert-error text-sm">Upload failed</div>';
      }
    });
};

/**
 * True when #options-pane exists and any option field differs from its original value.
 * Uses Alpine.$data(pane) (optionForm: values / originals / isAtOriginal).
 */
window.neoIsConfigFormDirty = function neoIsConfigFormDirty() {
  try {
    var pane = document.getElementById('options-pane');
    if (!pane) return false;
    if (typeof Alpine === 'undefined' || !Alpine.$data) return false;
    var data = Alpine.$data(pane);
    if (!data || !data.values || typeof data.isAtOriginal !== 'function') return false;
    var names = Object.keys(data.values);
    for (var i = 0; i < names.length; i++) {
      if (!data.isAtOriginal(names[i])) return true;
    }
    return false;
  } catch (e) {
    return false;
  }
};

/**
 * If the option form is dirty, ask the user to discard unsaved changes.
 * Returns true when navigation may proceed (not dirty, or user confirmed discard).
 */
window.neoConfirmLeaveConfigForm = function neoConfirmLeaveConfigForm() {
  if (!window.neoIsConfigFormDirty()) return true;
  return window.confirm('You have unsaved changes. Discard them?');
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
      if (this.tab === 'settings') return 'Settings';
      if (this.tab === 'versioning') return 'Versioning';
      if (this.tab === 'guides') return 'Guides';
      return 'Services';
    },

    setDetail(name) {
      this.detail = name || null;
    },

    clearDetail() {
      this.detail = null;
    },

    /** Canonical tab content URL under /configuration/... */
    tabUrl(tab) {
      if (tab === 'settings') return '/configuration/settings';
      if (tab === 'versioning') return '/configuration/versioning';
      if (tab === 'guides') return '/configuration/guides';
      // Prefer /configuration (not /configuration/services) as the services home URL.
      return '/configuration';
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
      if (!window.neoConfirmLeaveConfigForm()) return;
      this.tab = tab;
      this.navigateContent(this.tabUrl(tab));
    },

    /**
     * Leave the option pane and return to the parent grid for the current tab.
     * Breadcrumb stays visible until the grid swap lands (see syncConfigShellFromContent).
     */
    goUp() {
      if (!window.neoConfirmLeaveConfigForm()) return;
      this.navigateContent(this.tabUrl(this.tab));
    },

    /** Logo / home: services tab at /configuration. */
    goHome() {
      if (!window.neoConfirmLeaveConfigForm()) return;
      this.tab = 'services';
      this.navigateContent('/configuration');
    },
  };
};

/**
 * Alpine state for the Services tab: search + status / category / source filters.
 * Cards (`[data-svc]`) carry data-enabled / data-cat / data-plugin-urls / data-search.
 * Filters persist per browser tab (sessionStorage) so returning from a service keeps them.
 * Matching itself lives in services_filter.js (NeoServicesFilter), shared with its
 * Node unit tests, so the tab count badges reuse the exact same rules as the cards.
 */
window.servicesGrid = function servicesGrid() {
  var STORE_KEY = 'neo.servicesFilters';
  return {
    q: '',
    status: 'all', // all | installed | available
    category: 'all',
    plugin: 'all', // all | core | <plugin url>
    counts: { all: 0, installed: 0, available: 0 },

    init() {
      var cards = this.$root.querySelectorAll('[data-svc]');
      var installed = 0;
      cards.forEach(function (c) {
        if (c.dataset.enabled === 'true') installed++;
      });
      this.counts = { all: cards.length, installed: installed, available: cards.length - installed };
      try {
        var saved = JSON.parse(sessionStorage.getItem(STORE_KEY) || 'null');
        if (saved) {
          this.q = saved.q || '';
          this.status = saved.status || 'all';
          this.category = saved.category || 'all';
          this.plugin = saved.plugin || 'all';
        }
      } catch (e) {}
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
      try {
        sessionStorage.setItem(
          STORE_KEY,
          JSON.stringify({ q: this.q, status: this.status, category: this.category, plugin: this.plugin })
        );
      } catch (e) {}
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
        var urls = (c.dataset.pluginUrls || '').split('|').filter(Boolean);
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

  /** Instant navbar spinner for config navigations (no CSS fade). */
  function setNavBusy(on) {
    navLoadBusy = !!on;
    syncBarBusy();
  }

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

  /** Resolve #config-content whether detail.target is an element or a selector string. */
  function isConfigContentTarget(target) {
    if (!target) return false;
    if (typeof target === 'string') {
      return target === '#config-content' || target === 'config-content';
    }
    if (target.id === 'config-content') return true;
    // htmx sometimes reports the requesting elt; treat swaps into #config-content as nav.
    return !!(target.getAttribute && target.getAttribute('hx-target') === '#config-content');
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

  // Guard attribute-based (and other) HTMX navigations that replace #config-content.
  // Skip modals, action bar, unit controls, changes-body, etc. (other targets).
  // After a successful save, option_form updates originals before reload, so dirty is false.
  document.body.addEventListener('htmx:beforeRequest', function (evt) {
    try {
      var d = evt.detail || {};
      var t = d.target;
      var isNav = isConfigContentTarget(t) || isConfigContentTarget(d.elt);
      if (!isNav && d.requestConfig && d.requestConfig.target) {
        isNav = isConfigContentTarget(d.requestConfig.target);
      }
      if (!isNav) return;
      setNavBusy(true);
      lockConfigContentHeight();
      if (window.neoAllowNextConfigNav) {
        window.neoAllowNextConfigNav = false;
        return;
      }
      if (!window.neoConfirmLeaveConfigForm()) {
        setNavBusy(false);
        unlockConfigContentHeight();
        evt.preventDefault();
      }
    } catch (e) {}
  });

  // Always clear nav spinner when a config-content request finishes (or errors / aborts).
  function clearNavBusyIfConfig(evt) {
    try {
      var d = evt.detail || {};
      var t = d.target;
      var isNav = isConfigContentTarget(t) || isConfigContentTarget(d.elt);
      if (!isNav && d.requestConfig && d.requestConfig.target) {
        isNav = isConfigContentTarget(d.requestConfig.target);
      }
      if (!isNav) return;
      setNavBusy(false);
    } catch (e) {
      setNavBusy(false);
    }
  }
  document.body.addEventListener('htmx:afterRequest', clearNavBusyIfConfig);
  document.body.addEventListener('htmx:afterSwap', function (evt) {
    clearNavBusyIfConfig(evt);
    syncBarBusyIfActionBar(evt);
  });
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
  document.body.addEventListener('htmx:responseError', function (evt) {
    clearNavBusyIfConfig(evt);
    try {
      if (isConfigContentTarget((evt.detail || {}).target)) unlockConfigContentHeight();
    } catch (e) {
      unlockConfigContentHeight();
    }
  });
  document.body.addEventListener('htmx:sendError', function (evt) {
    clearNavBusyIfConfig(evt);
    try {
      if (isConfigContentTarget((evt.detail || {}).target)) unlockConfigContentHeight();
    } catch (e) {
      unlockConfigContentHeight();
    }
  });

  // Browser leave / hard navigation while the option form has unsaved edits.
  window.addEventListener('beforeunload', function (e) {
    try {
      if (!window.neoIsConfigFormDirty()) return;
      e.preventDefault();
      e.returnValue = '';
    } catch (err) {}
  });

  /** Sync shell.detail / shell.tab after HTMX swaps into #config-content. */
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

  function syncConfigShellFromContent() {
    try {
      var shell = document.getElementById('config-shell');
      if (!shell || typeof Alpine === 'undefined' || !Alpine.$data) return;
      var data = Alpine.$data(shell);
      if (!data) return;
      var content = document.getElementById('config-content');
      var pane = content && content.querySelector('#options-pane');
      var guide = content && content.querySelector('#guide-page');
      var privacy = content && content.querySelector('#privacy-map');
      if (pane) {
        data.setDetail(pane.getAttribute('data-service'));
        data.tab = pane.getAttribute('data-is-core') === 'true' ? 'settings' : 'services';
        focusOptionFromUrl(pane);
      } else if (guide) {
        data.setDetail(guide.getAttribute('data-guide-title') || 'Guide');
        data.tab = 'guides';
      } else if (privacy) {
        data.setDetail('Privacy map');
        data.tab = 'guides';
      } else {
        data.clearDetail();
        // Infer active tab from grid/branches partials (Back/Forward history restore).
        if (content && content.querySelector('#core-grid')) data.tab = 'settings';
        else if (
          content &&
          (content.querySelector('#versioning-root') ||
            content.querySelector('#branches-section'))
        )
          data.tab = 'versioning';
        else if (content && content.querySelector('#guides-grid')) data.tab = 'guides';
        else if (content && content.querySelector('#services-grid')) data.tab = 'services';
      }
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
      var ar = action.getBoundingClientRect();
      var sr = save.getBoundingClientRect();
      var stuck = sr.top <= stickyTop + 0.5;
      if (!stuck) {
        clearStickyBarsFuse();
        return;
      }
      root.classList.add('neo-bars-fused');
      if (!glass) return;
      // One plate from the top of the action bar to the bottom of the save strip,
      // matching the action bar's width so the fused chrome is continuous.
      glass.hidden = false;
      glass.style.top = ar.top + 'px';
      glass.style.left = ar.left + 'px';
      glass.style.width = ar.width + 'px';
      glass.style.height = Math.max(0, sr.bottom - ar.top) + 'px';
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
      if (
        t.id === 'config-content' ||
        (t.closest && t.closest('#config-content') && t.id === 'options-pane')
      ) {
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
  document.addEventListener('DOMContentLoaded', function () {
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

  // Live unit logs (SSE from /sse/logs/<unit>). The output panel is the same
  // logPane activation and update use, including Latest when follow is paused.
  window.currentLogSource = null;
  var unitLogsPane = null;

  function setLogConn(state) {
    var el = document.getElementById('log-conn');
    if (!el) return;
    var labels = { connecting: 'Connecting…', live: 'Live', reconnecting: 'Reconnecting…' };
    el.setAttribute('data-conn', state);
    el.textContent = labels[state] || '';
  }

  function ensureUnitLogsPane() {
    if (unitLogsPane) return unitLogsPane;
    var host = document.getElementById('logs-output');
    var api = window.NeoOpMonitor && window.NeoOpMonitor.logPane;
    if (!host || !api) return null;
    host.innerHTML = api.markup({ clear: true });
    unitLogsPane = api.bind(host.querySelector('.neo-op-output'), window);
    return unitLogsPane;
  }

  window.openUnitLogs = function (unit) {
    var dlg = document.getElementById('logs-dialog');
    var nameEl = document.getElementById('log-unit');
    var pane = ensureUnitLogsPane();
    if (!dlg || !nameEl || !pane) return;
    nameEl.textContent = unit;
    pane.clear();
    setLogConn('connecting');
    stopUnitLogs();
    dlg.showModal();
    pane.append('connecting live logs...\n');
    var es = new EventSource('/sse/logs/' + encodeURIComponent(unit));
    window.currentLogSource = es;
    es.onopen = function () { setLogConn('live'); };
    es.onmessage = function (e) {
      pane.append(e.data + '\n');
    };
    es.onerror = function () {
      setLogConn('reconnecting');
      pane.append('[live connection hiccup; may resume]\n');
    };
    dlg.dataset.unit = unit;
  };
  function stopUnitLogs() {
    if (!window.currentLogSource) return;
    var prev = window.currentLogSource;
    window.currentLogSource = null;
    prev.onopen = null;
    prev.onmessage = null;
    prev.onerror = null;
    try { prev.close(); } catch (e) {}
  }
  window.closeUnitLogs = function () {
    var dlg = document.getElementById('logs-dialog');
    stopUnitLogs();
    if (dlg) dlg.close();
  };
  window.clearUnitLogs = function () {
    var pane = ensureUnitLogsPane();
    if (pane) pane.clear();
  };
  var ld = document.getElementById('logs-dialog');
  if (ld) {
    ld.addEventListener('close', stopUnitLogs);
  }

  // Option helper form dialog (password/hash generators etc.)
  var helperDialogOnApply = null;
  window.openHelperDialog = function (helper, onApply) {
    var dlg = document.getElementById('helper-dialog');
    var title = document.getElementById('helper-dialog-title');
    var desc = document.getElementById('helper-dialog-desc');
    var fields = document.getElementById('helper-dialog-fields');
    var form = document.getElementById('helper-dialog-form');
    if (!dlg || !fields || !form) return;
    helperDialogOnApply = onApply;
    if (title) title.textContent = helper.label || 'Helper';
    if (desc) {
      desc.textContent = helper.description || '';
      desc.style.display = helper.description ? '' : 'none';
    }
    fields.innerHTML = '';
    (helper.inputs || []).forEach(function (inp) {
      var wrap = document.createElement('div');
      wrap.className = 'flex flex-col w-full';
      var lab = document.createElement('label');
      lab.className = 'label py-1';
      lab.innerHTML = '<span class="label-text text-sm">' +
        (inp.label || inp.name) +
        (inp.required === false ? '' : ' <span class="text-error">*</span>') +
        '</span>';
      wrap.appendChild(lab);
      var el;
      if (inp.type === 'bool') {
        el = document.createElement('input');
        el.type = 'checkbox';
        el.className = 'toggle toggle-primary toggle-sm';
        el.checked = !!inp.default;
      } else if (inp.type === 'enum' && inp.values && inp.values.length) {
        el = document.createElement('select');
        el.className = 'select select-sm w-full font-mono';
        inp.values.forEach(function (v) {
          var o = document.createElement('option');
          o.value = v;
          o.textContent = v;
          el.appendChild(o);
        });
      } else {
        el = document.createElement('input');
        el.type = inp.type === 'password' ? 'password' : (inp.type === 'int' ? 'number' : 'text');
        el.className = 'input input-sm w-full font-mono';
        if (inp.placeholder) el.placeholder = inp.placeholder;
        if (inp.type === 'password') el.autocomplete = 'new-password';
        if (inp.default != null) el.value = String(inp.default);
      }
      el.name = inp.name;
      el.dataset.helperInput = inp.name;
      el.dataset.helperType = inp.type || 'str';
      el.required = inp.required !== false;
      wrap.appendChild(el);
      fields.appendChild(wrap);
    });
    form.onsubmit = function (ev) {
      ev.preventDefault();
      var inputs = {};
      var nodes = fields.querySelectorAll('[data-helper-input]');
      for (var i = 0; i < nodes.length; i++) {
        var node = nodes[i];
        var name = node.dataset.helperInput;
        var t = node.dataset.helperType || 'str';
        if (t === 'bool') {
          inputs[name] = !!node.checked;
        } else if (t === 'int') {
          inputs[name] = node.value === '' ? null : Number(node.value);
        } else {
          inputs[name] = node.value;
        }
      }
      var cb = helperDialogOnApply;
      window.closeHelperDialog();
      if (typeof cb === 'function') cb(inputs);
      return false;
    };
    dlg.showModal();
    var first = fields.querySelector('input, select');
    if (first) setTimeout(function () { first.focus(); }, 50);
  };
  window.closeHelperDialog = function () {
    var dlg = document.getElementById('helper-dialog');
    var fields = document.getElementById('helper-dialog-fields');
    helperDialogOnApply = null;
    if (fields) fields.innerHTML = '';
    if (dlg) dlg.close();
  };
  var hd = document.getElementById('helper-dialog');
  if (hd) {
    hd.addEventListener('close', function () {
      helperDialogOnApply = null;
      var fields = document.getElementById('helper-dialog-fields');
      if (fields) fields.innerHTML = '';
    });
  }

  // Suppress noisy htmx swapErrors for OOB unit-controls updates when the row
  // is no longer in the DOM (e.g. switched panes, closed dialog, or rapid actions).
  // The status will be correct again on next pane load (which does a fresh bootstrap GET).
  document.body.addEventListener('htmx:swapError', function (evt) {
    try {
      var resp = evt.detail && evt.detail.xhr && evt.detail.xhr.responseText;
      if (resp && (/id="unit-controls-/.test(resp) || /id="update-out-/.test(resp))) {
        evt.stopImmediatePropagation();
      }
    } catch (e) {}
  });

  // Live unit status over /ws/status:
  // When an option pane with runtime units is shown, register those units with the
  // server so it polls systemctl ActiveState while this WebSocket stays open and
  // pushes OOB control HTML on every change. Leaving the pane (or disconnect)
  // clears interest so we do not poll forever.
  window.neoWsWrapper = null;
  window.neoWatchedUnits = [];

  function neoSendUnitWatch(op, units) {
    if (!window.neoWsWrapper || !units || !units.length) return;
    try {
      window.neoWsWrapper.send(JSON.stringify({ op: op, units: units }));
    } catch (e) {}
  }

  function neoCollectUnitsFrom(root) {
    if (!root || !root.querySelectorAll) return [];
    var rows = root.querySelectorAll('.unit-row[data-unit]');
    var out = [];
    for (var i = 0; i < rows.length; i++) {
      var n = rows[i].getAttribute('data-unit');
      if (n) out.push(n);
    }
    return out;
  }

  function neoSyncWatchedUnits(root) {
    var next = neoCollectUnitsFrom(root || document.getElementById('config-content'));
    // Replace interest set on the server (handles pane switches cleanly).
    neoSendUnitWatch('watch_replace', next);
    window.neoWatchedUnits = next;
  }

  document.body.addEventListener('htmx:wsOpen', function (evt) {
    try {
      window.neoWsWrapper = evt.detail && evt.detail.socketWrapper;
      // Re-register after reconnect (htmx ws extension reconnects automatically).
      if (window.neoWatchedUnits && window.neoWatchedUnits.length) {
        neoSendUnitWatch('watch_replace', window.neoWatchedUnits);
      } else {
        neoSyncWatchedUnits(document.getElementById('config-content'));
      }
    } catch (e) {}
  });

  document.body.addEventListener('htmx:wsClose', function () {
    window.neoWsWrapper = null;
  });

  document.body.addEventListener('htmx:afterSwap', function (evt) {
    try {
      var t = evt.detail && evt.detail.target;
      if (!t) return;
      // Option pane (or grid without units) landed in #config-content.
      if (
        t.id === 'config-content' ||
        (t.closest && t.closest('#config-content') && t.id === 'options-pane')
      ) {
        neoSyncWatchedUnits(document.getElementById('config-content'));
      }
    } catch (e) {}
  });

  // If the option pane was already present when the script ran (unlikely), sync once.
  document.addEventListener('DOMContentLoaded', function () {
    neoSyncWatchedUnits(document.getElementById('config-content'));
  });
})();

// Plugin pills: enable an ad-board ticker only when the label overflows one line.
(function pluginPills() {
  var observed = typeof WeakSet === 'function' ? new WeakSet() : null;

  function measure(pill) {
    var text = pill.querySelector('.plugin-pill-text');
    if (!text) return;
    var clip = pill.querySelector('.plugin-pill-clip') || pill;
    var box = clip.clientWidth;
    var overflow = text.offsetWidth > box + 0.5;
    if (overflow) {
      pill.classList.add('is-overflow');
      var seconds = Math.max(6, Math.min(18, text.offsetWidth / 24));
      pill.style.setProperty('--plugin-pill-duration', seconds.toFixed(1) + 's');
    } else {
      pill.classList.remove('is-overflow');
      pill.style.removeProperty('--plugin-pill-duration');
    }
  }

  var ro =
    typeof ResizeObserver === 'function'
      ? new ResizeObserver(function (entries) {
          for (var i = 0; i < entries.length; i++) {
            measure(entries[i].target);
          }
        })
      : null;

  function observe(root) {
    var scope = root && root.querySelectorAll ? root : document;
    var pills = scope.querySelectorAll('.plugin-pill');
    for (var i = 0; i < pills.length; i++) {
      var el = pills[i];
      if (ro && (!observed || !observed.has(el))) {
        if (observed) observed.add(el);
        ro.observe(el);
      }
      measure(el);
    }
  }

  function start() {
    observe(document);
    if (document.fonts && document.fonts.ready) {
      document.fonts.ready.then(function () {
        observe(document);
      });
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', start);
  } else {
    start();
  }

  document.body.addEventListener('htmx:afterSwap', function (evt) {
    var t = evt.detail && evt.detail.target;
    observe(t && t.querySelectorAll ? t : document);
  });
})();

// Unit status summary: folds the live per-unit ActiveState (pushed into
// #unit-controls-* over /ws/status) into the pane header badge and Status tab dot.
(function unitSummary() {
  var scheduled = false;

  function summarize() {
    scheduled = false;
    var root = document.getElementById('runtime-units');
    var badges = document.querySelectorAll('[data-unit-summary]');
    var dots = document.querySelectorAll('[data-unit-summary-dot]');
    if (!root || (!badges.length && !dots.length)) return;
    // Same summarizer as the services grid / sidebar dots (static/service_status.js).
    var ctrls = root.querySelectorAll('.unit-row .unit-controls');
    var entries = [];
    for (var i = 0; i < ctrls.length; i++) {
      entries.push({
        state: ctrls[i].getAttribute('data-active-state'),
        health: ctrls[i].getAttribute('data-health'),
        timer: ctrls[i].closest('.unit-row').hasAttribute('data-timer'),
      });
    }
    var summary = window.NeoUnitHealth
      ? window.NeoUnitHealth.summarize(entries)
      : { state: 'unknown', text: 'checking…' };
    for (var d = 0; d < dots.length; d++) {
      dots[d].setAttribute('data-state', summary.state);
      dots[d].setAttribute('title', summary.text);
    }
    for (var b = 0; b < badges.length; b++) {
      var t = badges[b].querySelector('[data-unit-summary-text]');
      if (t) t.textContent = summary.text;
    }
  }

  function schedule() {
    if (scheduled) return;
    scheduled = true;
    (window.requestAnimationFrame || setTimeout)(summarize);
  }

  function start() {
    var host = document.getElementById('config-content') || document.body;
    if (typeof MutationObserver === 'function') {
      new MutationObserver(schedule).observe(host, {
        subtree: true,
        childList: true,
        attributes: true,
        attributeFilter: ['data-active-state', 'data-health'],
      });
    }
    schedule();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', start);
  } else {
    start();
  }
})();
