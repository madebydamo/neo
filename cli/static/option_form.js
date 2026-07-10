// option_form.js
// Alpine form controller for service options + HTMX/Alpine re-init after swaps.
// Supports scalar fields, listOf/attrsOf of scalars, and one-deep submodule collections.

function optionForm() {
  return {
    values: {},
    defaults: {},
    originals: {},
    hadCurrent: {},
    optionsByName: {},
    serviceName: '',
    isCore: false,
    helperBusy: false,

    cloneValue(v) {
      if (v === null || v === undefined) return v;
      if (typeof structuredClone === 'function') {
        try { return structuredClone(v); } catch (_) { /* fall through */ }
      }
      try { return JSON.parse(JSON.stringify(v)); } catch (_) {
        if (Array.isArray(v)) return v.map((x) => this.cloneValue(x));
        if (v && typeof v === 'object') {
          const out = {};
          Object.keys(v).forEach((k) => { out[k] = this.cloneValue(v[k]); });
          return out;
        }
        return v;
      }
    },

    unwrapType(type) {
      if (!type) return type;
      if (type.kind === 'nullOr' && type.elem) return type.elem;
      return type;
    },

    defaultForType(type) {
      const t = this.unwrapType(type);
      if (!t || !t.kind) return null;
      switch (t.kind) {
        case 'bool':
          return false;
        case 'int':
        case 'port':
          return (t.min != null) ? t.min : 0;
        case 'float':
          return 0;
        case 'str':
        case 'path':
          return '';
        case 'enum':
          return (t.values && t.values.length) ? t.values[0] : '';
        case 'listOf':
          return [];
        case 'attrsOf':
          return {};
        case 'submodule': {
          const obj = {};
          (t.fields || []).forEach((f) => {
            if (f.default !== undefined && f.default !== null) {
              obj[f.name] = this.cloneValue(f.default);
            } else {
              obj[f.name] = this.defaultForType(f.type);
            }
          });
          return obj;
        }
        default:
          return null;
      }
    },

    mergeSubmoduleValue(val, elemType) {
      const base = this.defaultForType(elemType) || {};
      const v = (val && typeof val === 'object' && !Array.isArray(val))
        ? this.cloneValue(val)
        : {};
      Object.keys(base).forEach((k) => {
        if (v[k] === undefined) v[k] = base[k];
      });
      return v;
    },

    normalizeValue(opt, raw) {
      const type = opt.type || {};
      const kind = type.kind;
      let v = raw;
      if (v === undefined) v = null;

      if (kind === 'listOf') {
        if (!Array.isArray(v)) v = Array.isArray(opt.default) ? this.cloneValue(opt.default) : [];
        if (type.elem && type.elem.kind === 'submodule') {
          v = v.map((item) => this.mergeSubmoduleValue(item, type.elem));
        } else {
          v = this.cloneValue(v);
        }
        return v;
      }

      if (kind === 'attrsOf') {
        if (!v || typeof v !== 'object' || Array.isArray(v)) {
          v = (opt.default && typeof opt.default === 'object' && !Array.isArray(opt.default))
            ? this.cloneValue(opt.default)
            : {};
        } else {
          v = this.cloneValue(v);
        }
        if (type.elem && type.elem.kind === 'submodule') {
          const out = {};
          Object.keys(v).forEach((k) => {
            out[k] = this.mergeSubmoduleValue(v[k], type.elem);
          });
          return out;
        }
        return v;
      }

      return this.cloneValue(v);
    },

    initForm() {
      const raw = document.getElementById('options-seed')?.textContent || '[]';
      let opts = [];
      try { opts = JSON.parse(raw); } catch (e) { opts = []; }

      this.optionsByName = {};
      opts.forEach((o) => {
        this.optionsByName[o.name] = o;
        const hasCurrent = (o.current !== undefined && o.current !== null);
        const source = hasCurrent ? o.current : o.default;
        const v = this.normalizeValue(o, source);
        this.values[o.name] = v;
        this.defaults[o.name] = this.normalizeValue(o, o.default);
        this.originals[o.name] = this.cloneValue(v);
        this.hadCurrent[o.name] = hasCurrent;
      });

      const pane = document.getElementById('options-pane');
      this.serviceName = (pane?.dataset?.service)
        || (pane?.querySelector?.('h2')?.textContent?.trim())
        || '';
      this.isCore = (pane?.dataset?.isCore === 'true')
        || (pane?.dataset?.saveEndpoint || '').startsWith('/save-core/');
    },

    resolveHelper(optionName, target) {
      const opt = this.optionsByName[optionName];
      if (!opt) return null;
      if (target && target.field) {
        const fields = opt.type?.elem?.fields || [];
        const f = fields.find((x) => x.name === target.field);
        return f?.helper || null;
      }
      return opt.helper || null;
    },

    async runHelper(optionName, target) {
      const helper = this.resolveHelper(optionName, target);
      if (!helper) return;
      if (helper.kind === 'button') {
        return this.executeHelper(optionName, target, helper, {});
      }
      if (typeof window.openHelperDialog === 'function') {
        window.openHelperDialog(helper, (inputs) =>
          this.executeHelper(optionName, target, helper, inputs)
        );
      } else {
        alert('Helper dialog unavailable');
      }
    },

    async executeHelper(optionName, target, helper, inputs) {
      this.helperBusy = true;
      try {
        const res = await fetch('/helper/run', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            service: this.serviceName,
            option: optionName,
            is_core: !!this.isCore,
            target: target || null,
            inputs: inputs || {}
          })
        });
        const body = await res.json().catch(() => ({}));
        if (!body.ok) {
          alert(body.error || 'Helper failed');
          return;
        }
        this.applyHelperValue(
          optionName,
          target,
          helper.apply || body.apply || 'set',
          body.value
        );
      } catch (e) {
        alert('Helper error: ' + e);
      } finally {
        this.helperBusy = false;
      }
    },

    applyHelperValue(optionName, target, apply, value) {
      // Top-level listOf element: target = { index } without field (e.g. tinyauth users).
      if (target && target.index != null && target.index !== undefined && !target.field) {
        const list = this.ensureList(optionName);
        while (list.length <= target.index) {
          const elem = this.unwrapType(this.optType(optionName)?.elem);
          list.push(this.defaultForType(elem || { kind: 'str' }));
        }
        list[target.index] = value;
        this.values[optionName] = [...list];
        return;
      }
      if (!target || !target.field) {
        if (apply === 'append') {
          const list = this.ensureList(optionName);
          list.push(value);
          this.values[optionName] = [...list];
        } else {
          this.values[optionName] = value;
        }
        return;
      }
      if (target.key != null && target.key !== undefined) {
        const obj = this.ensureAttrs(optionName);
        const entry = Object.assign({}, obj[target.key] || {});
        if (apply === 'append') {
          const nested = Array.isArray(entry[target.field]) ? entry[target.field] : [];
          entry[target.field] = [...nested, value];
        } else {
          entry[target.field] = value;
        }
        obj[target.key] = entry;
        this.values[optionName] = { ...obj };
        return;
      }
      if (target.index != null && target.index !== undefined) {
        const entry = this.ensureListEntry(optionName, target.index);
        if (apply === 'append') {
          if (!Array.isArray(entry[target.field])) entry[target.field] = [];
          entry[target.field] = [...entry[target.field], value];
        } else {
          entry[target.field] = value;
        }
        this.values[optionName] = [...this.values[optionName]];
      }
    },

    optType(name) {
      return this.optionsByName[name]?.type || null;
    },

    fieldType(parentName, fieldName) {
      const fields = this.optType(parentName)?.elem?.fields || [];
      const f = fields.find((x) => x.name === fieldName);
      return f?.type || null;
    },

    resetField(name) {
      if (!name) return;
      const opt = this.optionsByName[name];
      if (opt) {
        this.values[name] = this.normalizeValue(opt, this.defaults[name]);
      } else {
        this.values[name] = this.cloneValue(this.defaults[name]);
      }
    },

    revertField(name) {
      if (!name) return;
      const origs = this.originals || {};
      if (!(name in origs)) return;
      this.values[name] = this.cloneValue(origs[name]);
    },

    resetAll() {
      Object.keys(this.defaults).forEach((k) => this.resetField(k));
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
      return this.isAtDefault(name) ? 'default' : 'modified';
    },

    ensureList(name) {
      if (!Array.isArray(this.values[name])) this.values[name] = [];
      return this.values[name];
    },

    ensureAttrs(name) {
      if (!this.values[name] || typeof this.values[name] !== 'object' || Array.isArray(this.values[name])) {
        this.values[name] = {};
      }
      return this.values[name];
    },

    /** Ordered keys for attrsOf editors (reactive via values[name] reassignment). */
    attrKeys(name) {
      return Object.keys(this.values[name] || {});
    },

    addListItem(name) {
      const list = this.ensureList(name);
      const elem = this.unwrapType(this.optType(name)?.elem);
      list.push(this.defaultForType(elem || { kind: 'str' }));
      this.values[name] = [...list];
    },

    removeListItem(name, idx) {
      const list = this.ensureList(name);
      list.splice(idx, 1);
      this.values[name] = [...list];
    },

    addAttrItem(name, inputEl) {
      const key = inputEl?.value?.trim();
      if (!key) return;
      const obj = this.ensureAttrs(name);
      if (Object.prototype.hasOwnProperty.call(obj, key)) return;
      const elem = this.unwrapType(this.optType(name)?.elem);
      obj[key] = this.defaultForType(elem || { kind: 'str' });
      this.values[name] = { ...obj };
      if (inputEl) inputEl.value = '';
    },

    removeAttrItem(name, key) {
      const obj = this.ensureAttrs(name);
      delete obj[key];
      this.values[name] = { ...obj };
    },

    renameAttrKey(name, oldKey, newKeyRaw) {
      const newKey = (newKeyRaw || '').trim();
      if (!newKey || newKey === oldKey) return;
      const obj = this.ensureAttrs(name);
      if (Object.prototype.hasOwnProperty.call(obj, newKey)) return;
      obj[newKey] = obj[oldKey];
      delete obj[oldKey];
      this.values[name] = { ...obj };
    },

    // Nested list inside attrsOf/listOf submodule entry
    ensureNestedList(parentName, entryKey, fieldName) {
      const parent = this.values[parentName];
      if (!parent || typeof parent !== 'object') return [];
      const entry = parent[entryKey];
      if (!entry || typeof entry !== 'object') return [];
      if (!Array.isArray(entry[fieldName])) entry[fieldName] = [];
      return entry[fieldName];
    },

    addNestedListItem(parentName, entryKey, fieldName) {
      const list = this.ensureNestedList(parentName, entryKey, fieldName);
      const ft = this.fieldType(parentName, fieldName);
      const elem = this.unwrapType(ft?.elem || ft);
      list.push(this.defaultForType(elem || { kind: 'str' }));
      // reassign for Alpine reactivity
      this.values[parentName] = { ...this.values[parentName] };
    },

    removeNestedListItem(parentName, entryKey, fieldName, idx) {
      const list = this.ensureNestedList(parentName, entryKey, fieldName);
      list.splice(idx, 1);
      this.values[parentName] = { ...this.values[parentName] };
    },

    // Nested attrsOf of scalars inside a submodule entry
    ensureNestedAttrs(parentName, entryKey, fieldName) {
      const parent = this.values[parentName];
      if (!parent || typeof parent !== 'object') return {};
      const entry = parent[entryKey];
      if (!entry || typeof entry !== 'object') return {};
      if (!entry[fieldName] || typeof entry[fieldName] !== 'object' || Array.isArray(entry[fieldName])) {
        entry[fieldName] = {};
      }
      return entry[fieldName];
    },

    addNestedAttrItem(parentName, entryKey, fieldName, inputEl) {
      const key = inputEl?.value?.trim();
      if (!key) return;
      const obj = this.ensureNestedAttrs(parentName, entryKey, fieldName);
      if (Object.prototype.hasOwnProperty.call(obj, key)) return;
      const ft = this.fieldType(parentName, fieldName);
      const elem = this.unwrapType(ft?.elem || { kind: 'str' });
      obj[key] = this.defaultForType(elem);
      this.values[parentName] = { ...this.values[parentName] };
      if (inputEl) inputEl.value = '';
    },

    removeNestedAttrItem(parentName, entryKey, fieldName, key) {
      const obj = this.ensureNestedAttrs(parentName, entryKey, fieldName);
      delete obj[key];
      this.values[parentName] = { ...this.values[parentName] };
    },

    // listOf submodule helpers (entry is index)
    ensureListEntry(parentName, idx) {
      const list = this.ensureList(parentName);
      while (list.length <= idx) {
        const elem = this.unwrapType(this.optType(parentName)?.elem);
        list.push(this.defaultForType(elem || { kind: 'submodule' }));
      }
      return list[idx];
    },

    addNestedListItemAtIndex(parentName, idx, fieldName) {
      const entry = this.ensureListEntry(parentName, idx);
      if (!Array.isArray(entry[fieldName])) entry[fieldName] = [];
      const ft = this.fieldType(parentName, fieldName);
      const elem = this.unwrapType(ft?.elem || { kind: 'str' });
      entry[fieldName].push(this.defaultForType(elem));
      this.values[parentName] = [...this.values[parentName]];
    },

    removeNestedListItemAtIndex(parentName, idx, fieldName, itemIdx) {
      const entry = this.ensureListEntry(parentName, idx);
      if (!Array.isArray(entry[fieldName])) return;
      entry[fieldName].splice(itemIdx, 1);
      this.values[parentName] = [...this.values[parentName]];
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
      Object.keys(origs).forEach((k) => this.revertField(k));
    },

    async save() {
      const svc = this.serviceName || 'service';
      const pane = document.getElementById('options-pane');
      const ep = (pane && pane.dataset && pane.dataset.saveEndpoint) || `/save/${encodeURIComponent(svc)}`;
      const toSave = {};
      Object.keys(this.values || {}).forEach((k) => {
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
          this.originals = {};
          Object.keys(this.values || {}).forEach((k) => {
            this.originals[k] = this.cloneValue(this.values[k]);
          });
          const orig = event?.target?.innerText;
          if (event?.target) event.target.innerText = 'Saved!';
          setTimeout(() => {
            if (event?.target) event.target.innerText = orig || 'Save';
          }, 1200);
          const loadUrl = pane?.dataset?.loadUrl;
          if (loadUrl) {
            htmx.ajax('GET', loadUrl, {target: '#config-content', swap: 'innerHTML'});
          }
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
