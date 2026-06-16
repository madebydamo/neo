// option_form.js
// Extracted from index.html.hbs: Alpine form controller for service options + HTMX/Alpine re-init after swaps.
// Loaded only by the configuration page.

function optionForm() {
  return {
    values: {},
    defaults: {},
    originals: {},
    hadCurrent: {},
    serviceName: '',

    initForm() {
      const raw = document.getElementById('options-seed')?.textContent || '[]';
      let opts = [];
      try { opts = JSON.parse(raw); } catch (e) { opts = []; }

      opts.forEach(o => {
        const hasCurrent = (o.current !== undefined && o.current !== null);
        let v = hasCurrent ? o.current : o.default;
        if (Array.isArray(v)) v = [...v];
        else if (v && typeof v === 'object') v = {...v};
        this.values[o.name] = v;
        this.defaults[o.name] = o.default;
        this.originals[o.name] = (Array.isArray(v) ? [...v] : (v && typeof v === 'object' ? {...v} : v));
        this.hadCurrent[o.name] = hasCurrent;
      });

      // Capture the service name at init time (right after HTMX outerHTML swap).
      // This is much more reliable than reading this.$el inside async handlers later.
      const pane = document.getElementById('options-pane');
      this.serviceName = (pane?.dataset?.service)
        || (pane?.querySelector?.('h2')?.textContent?.trim())
        || '';
    },

    resetField(name) {
      this.values[name] = this.defaults[name];
      if (Array.isArray(this.values[name])) this.values[name] = [...this.values[name]];
      else if (this.values[name] && typeof this.values[name] === 'object') this.values[name] = {...this.values[name]};
    },

    revertField(name) {
      if (!name) return;
      const origs = this.originals || {};
      if (!origs[name]) return;
      let v = origs[name];
      if (Array.isArray(v)) v = [...v];
      else if (v && typeof v === 'object' && v !== null) v = {...v};
      const vals = this.values || {};
      vals[name] = v;
    },

    resetAll() {
      Object.keys(this.defaults).forEach(k => {
        this.values[k] = this.defaults[k];
        if (Array.isArray(this.values[k])) this.values[k] = [...this.values[k]];
        else if (this.values[k] && typeof this.values[k] === 'object') this.values[k] = {...this.values[k]};
      });
    },

    deepEqual(a, b) {
      try { return JSON.stringify(a) === JSON.stringify(b); } catch (_) { return false; }
    },

    isAtDefault(name) {
      if (!name) return true;
      const vals = this.values || {};
      const defs = this.defaults || {};
      return this.deepEqual(vals[name], defs[name]);
    },

    isAtOriginal(name) {
      if (!name) return true;
      const vals = this.values || {};
      const origs = this.originals || {};
      return this.deepEqual(vals[name], origs[name]);
    },

    sourceLabel(name) {
      if (!name) return '';
      // Show "default" when the value equals the declared default (whether it came via current or not).
      // Show "modified" when it differs from the default.
      return this.isAtDefault(name) ? 'default' : 'modified';
    },

    addListItem(name) {
      if (!Array.isArray(this.values[name])) this.values[name] = [];
      this.values[name].push('');
    },

    removeListItem(name, idx) {
      if (Array.isArray(this.values[name])) this.values[name].splice(idx, 1);
    },

    addAttrItem(name, inputEl) {
      if (!this.values[name] || typeof this.values[name] !== 'object') this.values[name] = {};
      const key = inputEl?.value?.trim();
      if (key && !this.values[name].hasOwnProperty(key)) {
        this.values[name][key] = '';
        if (inputEl) inputEl.value = '';
      }
    },

    removeAttrItem(name, key) {
      if (this.values[name] && typeof this.values[name] === 'object') {
        delete this.values[name][key];
      }
    },

    logState() {
      const svc = this.serviceName || 'service';
      console.log('Current edited state for ' + svc, this.values);
    },

    copyJson() {
      const txt = JSON.stringify(this.values, null, 2);
      navigator.clipboard?.writeText(txt).then(() => {
        const orig = event?.target?.innerText;
        if (event?.target) event.target.innerText = 'Copied!';
        setTimeout(() => { if (event?.target) event.target.innerText = orig || 'Copy JSON'; }, 1200);
      }).catch(() => alert(txt));
    },

    revertAll() {
      const origs = this.originals || {};
      Object.keys(origs).forEach(k => {
        this.revertField(k);
      });
    },

    async save() {
      const svc = this.serviceName || 'service';
      const pane = document.getElementById('options-pane');
      const ep = (pane && pane.dataset && pane.dataset.saveEndpoint) || `/save/${encodeURIComponent(svc)}`;
      const toSave = {};
      Object.keys(this.values || {}).forEach(k => {
        if (!this.isAtDefault(k)) {
          toSave[k] = this.values[k];
        }
      });
      try {
        const res = await fetch(ep, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify(toSave)
        });
        if (res.ok) {
          // Sync originals so revert buttons disable and UI reflects the persisted state
          this.originals = {};
          Object.keys(this.values || {}).forEach(k => {
            let v = this.values[k];
            if (Array.isArray(v)) v = [...v];
            else if (v && typeof v === 'object' && v !== null) v = { ...v };
            this.originals[k] = v;
          });
          // Brief button feedback (uses implicit event from click context)
          const orig = event?.target?.innerText;
          if (event?.target) event.target.innerText = 'Saved!';
          setTimeout(() => {
            if (event?.target) event.target.innerText = orig || 'Save';
          }, 1200);
          const ind = document.getElementById('pending-changes');
          const loadUrl = pane?.dataset?.loadUrl;
          if (loadUrl) {
            htmx.ajax('GET', loadUrl, {target: '#config-content', swap: 'innerHTML'});
          }
          if (ind) htmx.ajax('GET', '/changes/indicator', {target: '#pending-changes', swap: 'innerHTML'});
        } else {
          const txt = await res.text().catch(() => '');
          alert('Save failed: ' + (txt || res.status));
        }
      } catch (e) {
        alert('Save error: ' + e);
      }
    }
  }
}

// Ensure Alpine picks up x-data etc. after HTMX outerHTML swaps of the options pane.
document.addEventListener('htmx:afterSettle', () => {
  const pane = document.getElementById('options-pane');
  if (pane && pane.hasAttribute('x-data') && typeof Alpine !== 'undefined') {
    Alpine.initTree(pane);
  }
});
