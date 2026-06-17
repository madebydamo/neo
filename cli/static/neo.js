// Multi-iframe navigator: each service gets its own iframe that we show/hide.
// This avoids full reloads when switching between apps.
let currentBlockedUrl = '';
let lastUrls = {};           // key -> last URL (for initial load + "open in new tab")
let currentKey = null;       // currently visible service key
const serviceIframes = {};   // key -> iframe element

// Persist last positions across dashboard reloads
try {
  const saved = localStorage.getItem('neo-last-urls');
  if (saved) lastUrls = JSON.parse(saved);
} catch (e) {}
function persistLastUrls() {
  try { localStorage.setItem('neo-last-urls', JSON.stringify(lastUrls)); } catch (e) {}
}

function getViewerHost() {
  return document.getElementById('viewer-host');
}

function updateWarmIndicators() {
  // Show green dot on all warm (pre-loaded) services, including the currently selected one.
  // Mark service buttons
  document.querySelectorAll('.svc-btn').forEach(btn => {
    const key = btn.dataset.sub;
    if (!key) return;
    if (serviceIframes[key]) {
      btn.classList.add('warm');
    } else {
      btn.classList.remove('warm');
    }
  });

  // Config button
  const cfgBtn = document.getElementById('config-btn');
  if (cfgBtn) {
    if (serviceIframes['__config']) {
      cfgBtn.classList.add('warm');
    } else {
      cfgBtn.classList.remove('warm');
    }
  }
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

function setActive(btn) {
  document.querySelectorAll('.svc-btn, #config-btn').forEach(b => b.classList.remove('active', 'text-primary-content'));
  if (btn) btn.classList.add('active', 'text-primary-content');
}

function hardEvictService(key) {
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

    const urlEl = document.getElementById('current-url');
    if (urlEl) urlEl.textContent = '';
    const status = document.getElementById('status');
    if (status) status.textContent = 'Ready';
  }
}

function hardResetCurrent() {
  if (!currentKey) return;
  const key = currentKey;
  const url = lastUrls[key];
  if (!url) return;

  hardEvictService(key);

  // Immediately recreate a fresh one
  setTimeout(() => {
    if (key === '__config') {
      const cfgBtn = document.getElementById('config-btn');
      loadConfig(cfgBtn);
    } else {
      // Parse subdomain + domain from the stored full URL
      try {
        const u = new URL(url);
        const parts = u.hostname.split('.');
        const subdomain = parts[0];
        const domain = parts.slice(1).join('.');
        const btn = document.querySelector(`.svc-btn[data-sub="${subdomain}"]`);
        loadService(subdomain, domain, btn || null);
      } catch (e) {
        console.warn('[neo] Could not hard reset service from URL', url, e);
      }
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

  // Set src only on first creation — this is the expensive step we now avoid on later switches
  iframe.src = targetUrl;

  host.appendChild(iframe);
  serviceIframes[key] = iframe;
  updateWarmIndicators();

  // Best-effort capture of deep URLs after loads (same-origin or permissive services)
  iframe.onload = () => {
    if (currentKey === key) {
      try {
        const loc = iframe.contentWindow?.location?.href;
        if (loc && !loc.startsWith('about:')) {
          lastUrls[key] = loc;
          const urlEl = document.getElementById('current-url');
          if (urlEl) urlEl.textContent = loc;
          persistLastUrls();
        }
      } catch (_) { /* cross-origin — normal for most external services */ }
    }
  };

  return iframe;
}

// Show exactly one iframe (or none), hide all others + overlays as appropriate
function showOnly(key) {
  const host = getViewerHost();
  const welcome = document.getElementById('welcome');
  const blocked = document.getElementById('embedding-blocked');

  // Hide all service iframes
  Object.values(serviceIframes).forEach(f => {
    if (f) f.style.display = 'none';
  });

  if (welcome) welcome.style.display = 'none';
  if (blocked) blocked.style.display = 'none';
  if (host) host.style.visibility = 'visible';

  if (key && serviceIframes[key]) {
    serviceIframes[key].style.display = '';
    currentKey = key;

    // Show the best-known URL for this view in the top bar (last captured or initial)
    const urlEl = document.getElementById('current-url');
    if (urlEl && lastUrls[key]) {
      urlEl.textContent = lastUrls[key];
    }
  } else {
    currentKey = null;
  }

  // Refresh dots: only non-current warm items should show the green dot
  updateWarmIndicators();
}

// Public handler for sidebar clicks (supports shift/ctrl/cmd+click to open in new tab)
function handleSidebarClick(e, subOrKey, domain, btn) {
  if (e.shiftKey || e.ctrlKey || e.metaKey) {
    // Modifier+click: open in new tab (like the top-right button)
    let url;
    if (subOrKey === '__config') {
      url = lastUrls['__config'] || '/configuration';
    } else {
      url = lastUrls[subOrKey] || (domain ? `https://${subOrKey}.${domain}/` : null);
    }
    if (url) window.open(url, '_blank');
    return;
  }

  if (subOrKey === '__config') {
    loadConfig(btn);
  } else {
    loadService(subOrKey, domain, btn);
  }
}

function loadService(subdomain, domain, btn) {
  const urlEl = document.getElementById('current-url');
  const status = document.getElementById('status');

  if (!domain || !subdomain) {
    alert('Missing domain or subdomain configuration');
    return;
  }

  const key = subdomain;
  const cached = lastUrls[key];
  const targetUrl = (cached && cached.startsWith('https://'))
    ? cached
    : `https://${subdomain}.${domain}/`;

  const svcName = (btn && btn.dataset && btn.dataset.name) || '';
  const isNeo = svcName.toLowerCase() === 'neo';

  hideAllOverlays();
  currentBlockedUrl = '';

  if (isNeo) {
    loadConfig(btn);
    return;
  }

  const iframeCompatibleAttr = (btn && btn.dataset && btn.dataset.iframeCompatible);
  const isIframeCompatible = iframeCompatibleAttr == null || iframeCompatibleAttr !== 'false';
  if (!isIframeCompatible) {
    showEmbeddingBlocked(targetUrl, svcName || subdomain, btn, key);
    return;
  }

  // Create the iframe for this service (src set only on first visit)
  const iframe = getOrCreateIframe(key, targetUrl);
  if (!iframe) return;

  // Remember what we loaded (for future switches and new-tab)
  lastUrls[key] = targetUrl;
  persistLastUrls();

  // Instant switch: just show the already-loaded (or newly created) iframe
  showOnly(key);

  urlEl.textContent = targetUrl;
  status.textContent = `Loading ${subdomain}...`;

  setActive(btn);

  // If it was newly created, the onload above will update status + capture URL
  // If it already existed, we may want an immediate status update
  if (iframe.style.display !== 'none') {
    status.textContent = subdomain;
  }
}

function loadConfig(btn) {
  const urlEl = document.getElementById('current-url');
  const status = document.getElementById('status');

  hideAllOverlays();
  currentBlockedUrl = '';

  const key = '__config';
  const cached = lastUrls[key] || '/configuration';

  const iframe = getOrCreateIframe(key, cached);
  if (!iframe) return;

  lastUrls[key] = cached;
  persistLastUrls();

  showOnly(key);

  urlEl.textContent = cached;
  status.textContent = 'Loading config editor...';

  setActive(btn);

  // onload handler on the iframe will fire for updates
}

function showEmbeddingBlocked(url, label, btn, key) {
  const blocked = document.getElementById('embedding-blocked');
  const urlEl = document.getElementById('current-url');
  const status = document.getElementById('status');
  const host = getViewerHost();

  // Hide any service iframes and welcome
  if (host) host.style.visibility = 'hidden';
  const welcome = document.getElementById('welcome');
  if (welcome) welcome.style.display = 'none';
  if (blocked) blocked.style.display = 'flex';

  document.getElementById('blocked-url').textContent = url;
  currentBlockedUrl = url;

  if (key) {
    lastUrls[key] = url;
    currentKey = key;
    persistLastUrls();
  }

  urlEl.textContent = url;
  status.textContent = `${label} (embedding protected)`;

  setActive(btn);

  // The blocked case "owns" the current view; update indicators (no dot on it)
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
  // Hard reload only the currently visible service (does not affect others)
  if (!currentKey) return;

  const iframe = serviceIframes[currentKey];
  if (iframe && iframe.src && !iframe.src.startsWith('about:')) {
    const currentSrc = iframe.src;
    iframe.src = currentSrc;   // triggers a real reload of just this iframe
    lastUrls[currentKey] = currentSrc;
    persistLastUrls();

    const status = document.getElementById('status');
    if (status) status.textContent = `Reloading ${currentKey}...`;
  }
}

function openInNewTab() {
  if (currentKey && serviceIframes[currentKey]) {
    const iframe = serviceIframes[currentKey];
    const url = lastUrls[currentKey] || iframe.src;
    if (url && !url.startsWith('about:')) {
      window.open(url, '_blank');
      lastUrls[currentKey] = url;
      persistLastUrls();
      return;
    }
  }
  if (currentBlockedUrl) {
    window.open(currentBlockedUrl, '_blank');
  }
}

// Keyboard hint: press / to focus first service
document.addEventListener('keydown', (e) => {
  if (e.key === '/' && document.activeElement.tagName === 'BODY') {
    e.preventDefault();
    const first = document.querySelector('.svc-btn');
    if (first) first.click();
  }
});

// Optional: start with welcome visible (no service loaded yet)
// (the initial HTML already shows it)

// Right-click on sidebar items: hard-evict (heavy reset) a warm service
const sidebar = document.querySelector('.w-16.bg-base-200');
if (sidebar) {
  sidebar.addEventListener('contextmenu', (e) => {
    const btn = e.target.closest('.svc-btn, #config-btn');
    if (!btn) return;

    const key = btn.dataset.sub || (btn.id === 'config-btn' ? '__config' : null);
    if (key && serviceIframes[key]) {
      e.preventDefault();
      hardEvictService(key);
    }
  });
}

// Initial indicator sync (in case of any pre-existing state)
updateWarmIndicators();
