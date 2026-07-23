// Multi-iframe navigator: each service gets its own iframe that we show/hide.
// Warm iframe → show/hide without reload. No warm iframe → open at the canonical URL.
// No last-URL persistence (localStorage); unload always means a fresh root open next time.
let currentBlockedUrl = '';
let currentKey = null;       // currently visible service key
const serviceIframes = {};   // key -> iframe element

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

// Drop legacy deep-link cache from older navigator builds (one-shot cleanup).
try { localStorage.removeItem('neo-last-urls'); } catch (e) {}

// --- Mobile collapsible sidebar (drawer) ---
// md+ keeps the sidebar always visible (static); below md it is an off-canvas drawer.
const MOBILE_NAV_MQ = '(max-width: 767px)';

function isMobileNav() {
  return window.matchMedia(MOBILE_NAV_MQ).matches;
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
window.matchMedia(MOBILE_NAV_MQ).addEventListener('change', (e) => {
  if (!e.matches) closeSidebar();
});

function getViewerHost() {
  return document.getElementById('viewer-host');
}

/** Best-effort URL for the top bar / open-in-new-tab (same-origin location, else iframe.src). */
function iframeDisplayUrl(iframe) {
  if (!iframe) return '';
  try {
    const loc = iframe.contentWindow?.location?.href;
    if (loc && !loc.startsWith('about:')) return loc;
  } catch (_) { /* cross-origin — normal for most external services */ }
  const src = iframe.src || '';
  return src.startsWith('about:') ? '' : src;
}

function setTopBarUrl(url) {
  const urlEl = document.getElementById('current-url');
  if (urlEl) urlEl.textContent = url || '';
}

function updateWarmIndicators() {
  // Show green dot on all warm (pre-loaded) services, including the currently selected one.
  document.querySelectorAll('.svc-btn').forEach(btn => {
    const key = btn.dataset.sub;
    if (!key) return;
    const checkKey = (key === 'neo') ? '__config' : key;
    if (serviceIframes[checkKey]) {
      btn.classList.add('warm');
    } else {
      btn.classList.remove('warm');
    }
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
  if (key === 'neo') key = '__config';
  const iframe = serviceIframes[key];
  if (iframe && iframe.parentNode) {
    iframe.parentNode.removeChild(iframe);
  }
  delete serviceIframes[key];
  updateWarmIndicators();

  if (currentKey === key) {
    currentKey = null;
    const host = getViewerHost();
    if (host) host.style.visibility = 'hidden';
    showWelcome();

    setTopBarUrl('');
    const status = document.getElementById('status');
    if (status) status.textContent = 'Ready';
  }
}

function hardResetCurrent() {
  if (!currentKey) return;
  const key = currentKey;

  hardEvictService(key);

  // Recreate at the canonical root (not a remembered deep path)
  setTimeout(() => {
    if (key === '__config') {
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
  iframe.sandbox = 'allow-same-origin allow-scripts allow-forms allow-popups allow-modals';
  iframe.style.display = 'none';

  // Set src only on first creation — this is the expensive step we avoid on later switches
  iframe.src = targetUrl;

  host.appendChild(iframe);
  serviceIframes[key] = iframe;
  updateWarmIndicators();

  // Update top bar after full loads (same-origin can show the real path)
  iframe.onload = () => {
    if (currentKey === key) {
      const loc = iframeDisplayUrl(iframe);
      if (loc) setTopBarUrl(loc);
      const status = document.getElementById('status');
      if (status && key === '__config') status.textContent = 'config';
      else if (status && key) status.textContent = key;
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

    const loc = iframeDisplayUrl(serviceIframes[key]);
    if (loc) setTopBarUrl(loc);
  } else {
    currentKey = null;
  }

  updateWarmIndicators();
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

/** URL for open-in-new-tab: warm iframe location if any, else canonical. */
function openTabUrlFor(subOrKey, domain) {
  const key = subOrKey === 'neo' ? '__config' : subOrKey;
  const warm = serviceIframes[key];
  if (warm) {
    const loc = iframeDisplayUrl(warm);
    if (loc) return loc;
  }
  if (subOrKey === 'neo') return configTargetUrl(subOrKey, domain);
  return serviceRootUrl(subOrKey, domain);
}

// Public handler for sidebar clicks (supports shift/ctrl/cmd+click to open in new tab)
function handleSidebarClick(e, subOrKey, domain, btn) {
  if (e.shiftKey || e.ctrlKey || e.metaKey) {
    const url = openTabUrlFor(subOrKey, domain);
    if (url) window.open(url, '_blank');
    return;
  }

  loadService(subOrKey, domain, btn);
  // Free up the viewport after picking a service on mobile.
  if (isMobileNav()) closeSidebar();
}

function loadService(subdomain, domain, btn) {
  const urlEl = document.getElementById('current-url');
  const status = document.getElementById('status');

  const key = subdomain;
  const svcName = (btn && btn.dataset && btn.dataset.name) || '';
  const isNeo = subdomain === 'neo' || svcName.toLowerCase() === 'neo';
  domain = domain || (btn && btn.dataset && btn.dataset.domain) || '';

  if (!subdomain || (!isNeo && !domain)) {
    alert('Missing domain or subdomain configuration');
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
    showEmbeddingBlocked(targetUrl, svcName || subdomain, btn, key);
    return;
  }

  // Create the iframe for this service (src set only on first visit)
  const iframe = getOrCreateIframe(key, targetUrl);
  if (!iframe) return;

  showOnly(key);

  const display = iframeDisplayUrl(iframe) || targetUrl;
  if (urlEl) urlEl.textContent = display;
  status.textContent = `Loading ${subdomain}...`;

  setActive(btn);

  if (iframe.style.display !== 'none') {
    status.textContent = subdomain;
  }
}

function loadConfig(btn, subdomain, domain) {
  const urlEl = document.getElementById('current-url');
  const status = document.getElementById('status');

  hideAllOverlays();
  currentBlockedUrl = '';

  subdomain = subdomain || (btn && btn.dataset && btn.dataset.sub) || 'neo';
  domain = domain || (btn && btn.dataset && btn.dataset.domain) || '';

  const key = '__config';
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

  const display = iframeDisplayUrl(iframe) || targetUrl;
  if (urlEl) urlEl.textContent = display;
  status.textContent = 'Loading config editor...';

  setActive(btn);
}

function showEmbeddingBlocked(url, label, btn, key) {
  const blocked = document.getElementById('embedding-blocked');
  const urlEl = document.getElementById('current-url');
  const status = document.getElementById('status');
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

  if (urlEl) urlEl.textContent = url;
  status.textContent = `${label} (embedding protected)`;

  setActive(btn);
  updateWarmIndicators();
}

function openBlockedInNewTab() {
  if (currentBlockedUrl) window.open(currentBlockedUrl, '_blank');
}

function copyBlockedUrl() {
  if (!currentBlockedUrl) return;
  navigator.clipboard?.writeText(currentBlockedUrl).catch(() => {});
}

function reloadFrame() {
  // Soft reload only the currently visible service (does not affect others)
  if (!currentKey) return;

  const iframe = serviceIframes[currentKey];
  if (iframe && iframe.src && !iframe.src.startsWith('about:')) {
    const currentSrc = iframe.src;
    iframe.src = currentSrc;
    setTopBarUrl(currentSrc);

    const status = document.getElementById('status');
    if (status) status.textContent = `Reloading ${currentKey}...`;
  }
}

function openInNewTab() {
  if (currentKey && serviceIframes[currentKey]) {
    const url = iframeDisplayUrl(serviceIframes[currentKey]);
    if (url) {
      window.open(url, '_blank');
      return;
    }
  }
  if (currentBlockedUrl) {
    window.open(currentBlockedUrl, '_blank');
  }
}

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

    let key = btn.dataset.sub;
    if (key === 'neo') key = '__config';
    if (key && serviceIframes[key]) {
      e.preventDefault();
      hardEvictService(key);
    }
  });
}

updateWarmIndicators();

// Sidebar service buttons are loaded via htmx after the page shell; re-sync warm dots then.
document.body.addEventListener('htmx:afterSwap', function (evt) {
  if (evt.detail && evt.detail.target && evt.detail.target.id === 'nav-services') {
    updateWarmIndicators();
  }
});
