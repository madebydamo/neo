// Multi-iframe navigator: each service gets its own iframe that we show/hide.
// Warm iframe → show/hide without reload. No warm iframe → open at the canonical URL.
// No last-URL persistence (localStorage); unload always means a fresh root open next time.
//
// Buttons in index.html.hbs carry data-neo-nav="<action>" (see ACTIONS below);
// sidebar items are `.svc-btn` links (nav_services.html.hbs). Both are handled by
// one delegated click listener, so nothing here is a global except NeoNav.
(function (root, factory) {
  var api = factory((root && root.NeoUtil) || require('./neo_util.js'));
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root && root.document) {
    root.NeoNav = api;
    api.autostart(root);
  }
})(typeof window !== 'undefined' ? window : null, function (NeoUtil) {
  'use strict';

  // md+ keeps the sidebar always visible (static); below md it is an off-canvas drawer.
  const MOBILE_NAV_MQ = '(max-width: 767px)';
  // The neo config editor's iframe key (its sidebar entry has data-sub="neo").
  const CONFIG_KEY = '__config';

  // --- Pure helpers (tested in neo.test.js) ---

  /** Iframe key for a sidebar subdomain. */
  function keyFor(sub) {
    return sub === 'neo' ? CONFIG_KEY : sub;
  }

  // Absolute URL for the neo config editor on the service subdomain.
  // Relative /configuration would stay on whatever host opened the navigator (e.g. a
  // custom top-level domain), where websockets/SSE often fail; neo.subdomain.domain works.
  function configTargetUrl(subdomain, domain) {
    const sub = subdomain || 'neo';
    if (domain) {
      return `https://${sub}.${domain}/configuration`;
    }
    return '/configuration';
  }

  function serviceRootUrl(subdomain, domain) {
    if (!subdomain || !domain) return null;
    return `https://${subdomain}.${domain}/`;
  }

  /** A click the browser should handle itself (new tab / window / download). */
  function isModifiedClick(e) {
    return !!(e.shiftKey || e.ctrlKey || e.metaKey || e.altKey || e.button > 0);
  }

  /**
   * Best-effort URL for the top bar / open-in-new-tab / copy.
   * Preference order: a live same-origin read (freshest — only ever works for
   * services sharing our exact origin, essentially never true across subdomains),
   * then the last URL the iframe told us about via postMessage (`known`), then
   * the src we last navigated it to.
   */
  function iframeDisplayUrl(iframe, known) {
    if (!iframe) return '';
    try {
      const loc = iframe.contentWindow?.location?.href;
      if (loc && !loc.startsWith('about:')) return loc;
    } catch (_) { /* cross-origin — normal for most external services */ }
    if (known) return known;
    const src = iframe.src || '';
    return src.startsWith('about:') ? '' : src;
  }

  /**
   * The href a `neo-iframe-location` message reports, or null when the message
   * is not one or its origin is not the one we navigated the iframe to (`src`,
   * resolved against `base`).
   */
  function reportedLocation(data, origin, src, base) {
    if (!data || data.source !== 'neo-iframe-location' || typeof data.href !== 'string') return null;
    let expectedOrigin = '';
    try { expectedOrigin = new URL(src, base).origin; } catch (_) { /* ignore */ }
    if (!expectedOrigin || origin !== expectedOrigin) return null;
    return data.href;
  }

  // --- Navigator state, bound to one window by autostart ---

  function autostart(win) {
    const document = win.document;
    let currentBlockedUrl = '';
    let currentKey = null;       // currently visible service key
    const serviceIframes = {};   // key -> iframe element
    // Best-effort "deepest known URL" per key. Filled from a same-origin location
    // read (rare — services live on other subdomains) or from a postMessage the
    // embedded page sends us (see the `message` listener below). Falls back to the
    // iframe's `src` when neither is available, e.g. for the many third-party apps
    // that do not know about neo. Never persisted (no long-lived deep-link cache).
    const iframeKnownUrl = {};

    NeoUtil.markStandalone();

    // Drop legacy deep-link cache from older navigator builds (one-shot cleanup).
    NeoUtil.store('local').remove('neo-last-urls');

    // --- Mobile collapsible sidebar (drawer) ---

    function isMobileNav() {
      return win.matchMedia(MOBILE_NAV_MQ).matches;
    }

    function openSidebar() {
      const sidebar = document.getElementById('sidebar');
      const backdrop = document.getElementById('sidebar-backdrop');
      const toggle = document.getElementById('sidebar-toggle');
      if (!sidebar) return;
      sidebar.dataset.open = 'true';
      sidebar.classList.remove('-translate-x-full');
      sidebar.classList.add('translate-x-0');
      if (backdrop) backdrop.classList.remove('hidden');
      if (toggle) toggle.setAttribute('aria-expanded', 'true');
    }

    function closeSidebar() {
      const sidebar = document.getElementById('sidebar');
      const backdrop = document.getElementById('sidebar-backdrop');
      const toggle = document.getElementById('sidebar-toggle');
      if (!sidebar) return;
      sidebar.dataset.open = 'false';
      // On desktop the md:translate-x-0 utility keeps it visible; only hide off-canvas on mobile.
      sidebar.classList.add('-translate-x-full');
      sidebar.classList.remove('translate-x-0');
      if (backdrop) backdrop.classList.add('hidden');
      if (toggle) toggle.setAttribute('aria-expanded', 'false');
    }

    function toggleSidebar() {
      const sidebar = document.getElementById('sidebar');
      if (sidebar && sidebar.dataset.open === 'true') closeSidebar();
      else openSidebar();
    }

    // Keep drawer state sane when rotating / resizing past the md breakpoint.
    win.matchMedia(MOBILE_NAV_MQ).addEventListener('change', (e) => {
      if (!e.matches) closeSidebar();
    });

    function getViewerHost() {
      return document.getElementById('viewer-host');
    }

    function displayUrl(key) {
      return iframeDisplayUrl(serviceIframes[key], key && iframeKnownUrl[key]);
    }

    function setTopBarUrl(url) {
      const urlEl = document.getElementById('current-url');
      if (!urlEl) return;
      urlEl.value = url || '';
      // `<input>` has no child text nodes for a CSS :empty check to see — toggle a
      // class instead so the pill still hides until a service has loaded.
      const pill = urlEl.closest('.neo-url-pill');
      if (pill) pill.classList.toggle('is-empty', !url);
    }

    /**
     * Cross-origin deep-link tracking.
     *
     * Services live on their own subdomains, so `iframe.contentWindow.location` is
     * blocked by the same-origin policy almost everywhere (the try/catch in
     * iframeDisplayUrl is the whole story for same-origin). There is no generic,
     * unprivileged way for a parent page to observe navigation inside a
     * cross-origin iframe.
     *
     * This listener implements the receiving half of a small opt-in contract: an
     * embedded page (or a script injected by the reverse proxy) may
     *   window.parent.postMessage({ source: 'neo-iframe-location', href: location.href }, '*')
     * on load/popstate/hashchange/pushState/replaceState, and we will reflect that
     * into the top bar. Nothing sends this message today — a generic SWAG-side
     * injection (nginx `sub_filter` adding this snippet to every proxied text/html
     * response) was considered but deliberately not built here: it touches the
     * shared reverse-proxy config every service goes through, and whether the SWAG
     * image's nginx even has `ngx_http_sub_filter_module` compiled in could not be
     * verified without booting the VM. A wrong shared change there takes every
     * proxied vhost down (see AGENTS.md's SWAG traps), so it needs its own,
     * VM-tested change. The contract itself is generic and forward-compatible in
     * the meantime: any service (or a per-app userscript) that adopts it starts
     * working with no further changes here.
     *
     * Validation: `event.source` must be the contentWindow of one of *our* tracked
     * iframes (this is a browser-guaranteed identity, not attacker-controllable —
     * postMessage's `source` cannot be spoofed by the sender), and `event.origin`
     * must exactly match the origin we ourselves navigated that iframe to.
     */
    win.addEventListener('message', (e) => {
      const data = e && e.data;
      if (!data || data.source !== 'neo-iframe-location') return;
      for (const key in serviceIframes) {
        const iframe = serviceIframes[key];
        if (!iframe || iframe.contentWindow !== e.source) continue;
        const href = reportedLocation(data, e.origin, iframe.src, win.location.href);
        if (href == null) return;
        iframeKnownUrl[key] = href;
        if (currentKey === key) setTopBarUrl(href);
        return;
      }
    });

    function updateWarmIndicators() {
      // Show green dot on all warm (pre-loaded) services, including the currently selected one.
      document.querySelectorAll('.svc-btn').forEach(btn => {
        const sub = btn.dataset.sub;
        if (!sub) return;
        btn.classList.toggle('warm', !!serviceIframes[keyFor(sub)]);
        refreshSidebarHref(btn);
      });
    }

    function setActive(btn) {
      document.querySelectorAll('.svc-btn').forEach(b => b.classList.remove('active', 'text-primary-content'));
      if (btn) btn.classList.add('active', 'text-primary-content');
    }

    function hideAllOverlays() {
      const welcome = document.getElementById('welcome');
      const blocked = document.getElementById('embedding-blocked');
      if (welcome) welcome.style.display = 'none';
      if (blocked) blocked.style.display = 'none';
    }

    function showWelcome() {
      const welcome = document.getElementById('welcome');
      const blocked = document.getElementById('embedding-blocked');
      const host = getViewerHost();
      if (welcome) welcome.style.display = '';
      if (blocked) blocked.style.display = 'none';
      if (host) host.style.visibility = 'hidden';
    }

    function hardEvictService(key) {
      key = keyFor(key);
      const iframe = serviceIframes[key];
      if (iframe && iframe.parentNode) {
        iframe.parentNode.removeChild(iframe);
      }
      delete serviceIframes[key];
      delete iframeKnownUrl[key];
      updateWarmIndicators();

      if (currentKey === key) {
        currentKey = null;
        const host = getViewerHost();
        if (host) host.style.visibility = 'hidden';
        showWelcome();

        setTopBarUrl('');
      }
    }

    function hardResetCurrent() {
      if (!currentKey) return;
      const key = currentKey;

      hardEvictService(key);

      // Recreate at the canonical root (not a remembered deep path)
      setTimeout(() => {
        if (key === CONFIG_KEY) {
          const btn = document.querySelector('.svc-btn[data-sub="neo"]');
          const sub = (btn && btn.dataset && btn.dataset.sub) || 'neo';
          const domain = (btn && btn.dataset && btn.dataset.domain) || '';
          loadConfig(btn, sub, domain);
        } else {
          const btn = document.querySelector(`.svc-btn[data-sub="${key}"]`);
          const domain = (btn && btn.dataset && btn.dataset.domain) || '';
          loadService(key, domain, btn || null);
        }
      }, 10);
    }

    // Create (or return existing) iframe for a service/config key. Src is only set once.
    function getOrCreateIframe(key, targetUrl) {
      if (serviceIframes[key]) {
        return serviceIframes[key];
      }

      const host = getViewerHost();
      if (!host) return null;

      const iframe = document.createElement('iframe');
      iframe.id = `iframe-${key.replace(/[^a-z0-9_-]/gi, '')}`;
      iframe.className = 'absolute inset-0 w-full h-full border-0 bg-white';
      // allow-downloads: settings.toml (and other in-app downloads) are blocked in
      // sandboxed iframes otherwise — the click either no-ops or navigates the frame.
      iframe.sandbox =
        'allow-same-origin allow-scripts allow-forms allow-popups allow-modals allow-downloads';
      iframe.style.display = 'none';

      // Set src only on first creation — this is the expensive step we avoid on later switches
      iframe.src = targetUrl;
      iframeKnownUrl[key] = targetUrl;

      host.appendChild(iframe);
      serviceIframes[key] = iframe;
      updateWarmIndicators();

      // Update top bar after full loads (same-origin can show the real path)
      iframe.onload = () => {
        if (currentKey === key) {
          const loc = displayUrl(key);
          if (loc) setTopBarUrl(loc);
        }
      };

      return iframe;
    }

    // Show exactly one iframe (or none), hide all others + overlays as appropriate
    function showOnly(key) {
      const host = getViewerHost();
      const welcome = document.getElementById('welcome');
      const blocked = document.getElementById('embedding-blocked');

      Object.values(serviceIframes).forEach(f => {
        if (f) f.style.display = 'none';
      });

      if (welcome) welcome.style.display = 'none';
      if (blocked) blocked.style.display = 'none';
      if (host) host.style.visibility = 'visible';

      if (key && serviceIframes[key]) {
        serviceIframes[key].style.display = '';
        currentKey = key;

        const loc = displayUrl(key);
        if (loc) setTopBarUrl(loc);
      } else {
        currentKey = null;
      }

      updateWarmIndicators();
    }

    /** URL for open-in-new-tab: warm iframe location if any, else canonical. */
    function openTabUrlFor(subOrKey, domain) {
      const key = keyFor(subOrKey);
      if (serviceIframes[key]) {
        const loc = displayUrl(key);
        if (loc) return loc;
      }
      if (subOrKey === 'neo') return configTargetUrl(subOrKey, domain);
      return serviceRootUrl(subOrKey, domain);
    }

    // Sidebar items are real <a href> links: ctrl/cmd/shift/middle click, long-press
    // menus and drag-to-tab are left to the browser, which opens them the same way
    // in every browser (a window.open() from a modified click is not: Safari ignored
    // it). refreshSidebarHref keeps the href on the warm iframe's URL.
    function refreshSidebarHref(btn) {
      if (!btn || !btn.dataset) return;
      const url = openTabUrlFor(btn.dataset.sub, btn.dataset.domain);
      if (url) btn.setAttribute('href', url);
      else btn.removeAttribute('href');
    }

    // Sidebar clicks: a plain click loads the service in the viewer.
    function handleSidebarClick(e, btn) {
      if (isModifiedClick(e)) {
        refreshSidebarHref(btn);
        return;
      }
      e.preventDefault();

      loadService(btn.dataset.sub, btn.dataset.domain, btn);
      // Free up the viewport after picking a service on mobile.
      if (isMobileNav()) closeSidebar();
    }

    function loadService(subdomain, domain, btn) {
      const key = subdomain;
      const svcName = (btn && btn.dataset && btn.dataset.name) || '';
      const isNeo = subdomain === 'neo' || svcName.toLowerCase() === 'neo';
      domain = domain || (btn && btn.dataset && btn.dataset.domain) || '';

      if (!subdomain || (!isNeo && !domain)) {
        win.NeoUtil.alert('Missing domain or subdomain configuration');
        return;
      }

      hideAllOverlays();
      currentBlockedUrl = '';

      if (isNeo) {
        loadConfig(btn, subdomain, domain);
        return;
      }

      const targetUrl = serviceRootUrl(subdomain, domain);

      const iframeCompatibleAttr = (btn && btn.dataset && btn.dataset.iframeCompatible);
      const isIframeCompatible = iframeCompatibleAttr == null || iframeCompatibleAttr !== 'false';
      if (!isIframeCompatible) {
        showEmbeddingBlocked(targetUrl, btn, key);
        return;
      }

      // Create the iframe for this service (src set only on first visit)
      const iframe = getOrCreateIframe(key, targetUrl);
      if (!iframe) return;

      showOnly(key);

      setTopBarUrl(displayUrl(key) || targetUrl);

      setActive(btn);
    }

    function loadConfig(btn, subdomain, domain) {
      hideAllOverlays();
      currentBlockedUrl = '';

      subdomain = subdomain || (btn && btn.dataset && btn.dataset.sub) || 'neo';
      domain = domain || (btn && btn.dataset && btn.dataset.domain) || '';

      const key = CONFIG_KEY;
      const targetUrl = configTargetUrl(subdomain, domain);

      // Drop a warm iframe that still points at the wrong origin (e.g. relative
      // /configuration resolved against a custom top-level domain).
      const existing = serviceIframes[key];
      if (existing && subdomain && domain) {
        const expected = `https://${subdomain}.${domain}`;
        const src = existing.src || '';
        if (src && !src.startsWith(expected)) {
          hardEvictService(key);
        }
      }

      const iframe = getOrCreateIframe(key, targetUrl);
      if (!iframe) return;

      showOnly(key);

      setTopBarUrl(displayUrl(key) || targetUrl);

      setActive(btn);
    }

    function showEmbeddingBlocked(url, btn, key) {
      const blocked = document.getElementById('embedding-blocked');
      const host = getViewerHost();

      if (host) host.style.visibility = 'hidden';
      const welcome = document.getElementById('welcome');
      if (welcome) welcome.style.display = 'none';
      if (blocked) blocked.style.display = 'flex';

      document.getElementById('blocked-url').textContent = url;
      currentBlockedUrl = url;

      if (key) {
        currentKey = key;
      }

      setTopBarUrl(url);

      setActive(btn);
      updateWarmIndicators();
    }

    function openBlockedInNewTab() {
      if (currentBlockedUrl) win.open(currentBlockedUrl, '_blank');
    }

    function copyBlockedUrl() {
      if (!currentBlockedUrl) return;
      win.navigator.clipboard?.writeText(currentBlockedUrl).catch(() => {});
    }

    function reloadFrame() {
      // Soft reload only the currently visible service (does not affect others)
      if (!currentKey) return;

      const iframe = serviceIframes[currentKey];
      if (iframe && iframe.src && !iframe.src.startsWith('about:')) {
        const currentSrc = iframe.src;
        iframe.src = currentSrc;
        // Reloading always returns to the canonical root (iframe.src never tracks
        // an in-app deep link — see the module comment), so the known URL resets too.
        iframeKnownUrl[currentKey] = currentSrc;
        setTopBarUrl(currentSrc);
      }
    }

    function openInNewTab() {
      if (currentKey && serviceIframes[currentKey]) {
        const url = displayUrl(currentKey);
        if (url) {
          win.open(url, '_blank');
          return;
        }
      }
      if (currentBlockedUrl) {
        win.open(currentBlockedUrl, '_blank');
      }
    }

    // data-neo-nav="<action>" on an element runs one of these on click.
    const ACTIONS = {
      'close-sidebar': closeSidebar,
      'toggle-sidebar': toggleSidebar,
      'reload': reloadFrame,
      'hard-reset': hardResetCurrent,
      'open-tab': openInNewTab,
      'open-blocked': openBlockedInNewTab,
      'copy-blocked': copyBlockedUrl,
    };

    // Sidebar items arrive via htmx and are moved around by sidebar_layout.js, so
    // clicks are delegated. sidebar_layout's capture listener on #sidebar stops the
    // click that ends a drag before it gets here.
    document.addEventListener('click', (e) => {
      const target = e.target;
      if (!target || !target.closest) return;
      const action = target.closest('[data-neo-nav]');
      if (action && ACTIONS[action.dataset.neoNav]) {
        ACTIONS[action.dataset.neoNav]();
        return;
      }
      const btn = target.closest('.svc-btn');
      if (btn && btn.dataset.sub) handleSidebarClick(e, btn);
    });

    // Keyboard: / focuses first service; Escape closes mobile drawer
    document.addEventListener('keydown', (e) => {
      if (e.key === 'Escape' && isMobileNav()) {
        closeSidebar();
        return;
      }
      if (e.key === '/' && document.activeElement.tagName === 'BODY') {
        e.preventDefault();
        if (isMobileNav()) openSidebar();
        const first = document.querySelector('.svc-btn');
        if (first) first.click();
      }
    });

    // Right-click on sidebar items: hard-evict (unload) a warm service
    const sidebar = document.getElementById('sidebar');
    if (sidebar) {
      sidebar.addEventListener('contextmenu', (e) => {
        const btn = e.target.closest('.svc-btn');
        if (!btn) return;
        // macOS ctrl+click is a right click: show the link menu (Open in New Tab).
        if (e.ctrlKey) return;

        const key = btn.dataset.sub && keyFor(btn.dataset.sub);
        if (key && serviceIframes[key]) {
          e.preventDefault();
          hardEvictService(key);
        }
      });
      // Middle click opens the link natively, but only if the href is current.
      sidebar.addEventListener('pointerdown', (e) => {
        refreshSidebarHref(e.target.closest('.svc-btn'));
      });
    }

    updateWarmIndicators();

    // Sidebar service buttons are loaded via htmx after the page shell; re-sync warm dots then.
    document.body.addEventListener('htmx:afterSwap', function (evt) {
      if (evt.detail && evt.detail.target && evt.detail.target.id === 'nav-services') {
        updateWarmIndicators();
      }
    });

    // Top-bar URL: select the whole value on tap/focus so the native copy pop-up
    // appears (readonly input — iOS/Android both offer Copy on a text selection).
    const urlEl = document.getElementById('current-url');
    if (urlEl) {
      const selectAll = () => {
        try { urlEl.select(); } catch (_) {}
      };
      urlEl.addEventListener('focus', selectAll);
      urlEl.addEventListener('click', selectAll);
    }
  }

  return {
    keyFor: keyFor,
    configTargetUrl: configTargetUrl,
    serviceRootUrl: serviceRootUrl,
    isModifiedClick: isModifiedClick,
    iframeDisplayUrl: iframeDisplayUrl,
    reportedLocation: reportedLocation,
    autostart: autostart,
  };
});
