// Shared test host for colocated widget scripts.
// Widgets register on globalThis.NeoWidgets; this builds a form-shaped object
// with the host helpers those widgets call, without Alpine or option_form.js.
'use strict';

function cloneValue(v) {
  if (v === null || v === undefined) return v;
  if (typeof structuredClone === 'function') {
    try { return structuredClone(v); } catch (_) { /* fall through */ }
  }
  try { return JSON.parse(JSON.stringify(v)); } catch (_) {
    if (Array.isArray(v)) return v.map((x) => cloneValue(x));
    if (v && typeof v === 'object') {
      const out = {};
      Object.keys(v).forEach((k) => { out[k] = cloneValue(v[k]); });
      return out;
    }
    return v;
  }
}

function unwrapType(type) {
  if (!type) return type;
  if (type.kind === 'nullOr' && type.elem) return type.elem;
  return type;
}

function defaultForType(type) {
  const t = unwrapType(type);
  if (!t || !t.kind) return null;
  switch (t.kind) {
    case 'bool': return false;
    case 'int':
    case 'port': return (t.min != null) ? t.min : 0;
    case 'float': return 0;
    case 'str':
    case 'path': return '';
    case 'enum': return (t.values && t.values.length) ? t.values[0] : '';
    case 'listOf': return [];
    case 'attrsOf': return {};
    case 'submodule': {
      const obj = {};
      (t.fields || []).forEach((f) => {
        if (f.default !== undefined && f.default !== null) {
          obj[f.name] = cloneValue(f.default);
        } else {
          obj[f.name] = defaultForType(f.type);
        }
      });
      return obj;
    }
    default: return null;
  }
}

function mergeSubmoduleValue(val, elemType) {
  const base = defaultForType(elemType) || {};
  const v = (val && typeof val === 'object' && !Array.isArray(val))
    ? cloneValue(val)
    : {};
  Object.keys(base).forEach((k) => {
    if (v[k] === undefined) v[k] = base[k];
  });
  return v;
}

function createFakeElement(id, extras) {
  extras = extras || {};
  const listeners = new Map();
  const el = {
    id,
    innerHTML: extras.innerHTML || '',
    textContent: extras.textContent != null ? extras.textContent : '',
    className: extras.className || '',
    open: false,
    value: extras.value || '',
    dataset: extras.dataset || {},
    addEventListener(type, fn) {
      if (!listeners.has(type)) listeners.set(type, new Set());
      listeners.get(type).add(fn);
    },
    removeEventListener(type, fn) {
      listeners.get(type)?.delete(fn);
    },
    dispatchEvent(ev) {
      const type = typeof ev === 'string' ? ev : ev.type;
      for (const fn of listeners.get(type) || []) fn(ev);
    },
    showModal() { el.open = true; },
    close() {
      el.open = false;
      el.dispatchEvent({ type: 'close' });
    },
    click() { el.dispatchEvent({ type: 'click' }); },
  };
  return el;
}

function createDocument(seed) {
  seed = seed || {};
  const elements = new Map();
  const doc = {
    elements,
    addEventListener() {},
    getElementById(id) {
      if (elements.has(id)) return elements.get(id);
      return null;
    },
    set(id, extras) {
      const el = createFakeElement(id, extras);
      elements.set(id, el);
      return el;
    },
  };
  if (seed.pluginInventory != null) {
    doc.set('plugin-inventory-seed', {
      textContent: typeof seed.pluginInventory === 'string'
        ? seed.pluginInventory
        : JSON.stringify(seed.pluginInventory),
    });
  }
  if (seed.pane) {
    doc.set('options-pane', {
      dataset: {
        service: seed.pane.service || '',
        isCore: seed.pane.isCore ? 'true' : 'false',
        saveEndpoint: seed.pane.saveEndpoint || '',
      },
    });
  }
  return doc;
}

function installGlobals(opts) {
  opts = opts || {};
  const document = opts.document || createDocument(opts);
  globalThis.document = document;
  if (!globalThis.window) globalThis.window = globalThis;
  globalThis.window.confirm = opts.confirm || (() => true);
  globalThis.window.neoToast = opts.neoToast || (() => {});
  if (opts.fetch) {
    globalThis.fetch = opts.fetch;
  }
  return document;
}

function extractKeyFromItem(item, extract) {
  const s = String(item ?? '');
  if (extract === 'beforeColon') {
    const i = s.indexOf(':');
    return i >= 0 ? s.slice(0, i) : s;
  }
  return s;
}

function makeForm(opts) {
  opts = opts || {};
  const NeoWidgets = globalThis.NeoWidgets;
  const document = installGlobals(opts);

  const host = {
    values: opts.values ? cloneValue(opts.values) : {},
    defaults: opts.defaults ? cloneValue(opts.defaults) : {},
    originals: opts.originals ? cloneValue(opts.originals) : {},
    hadCurrent: {},
    optionsByName: {},
    serviceName: opts.serviceName || 'svc',
    isCore: !!opts.isCore,
    uiState: {},
    plDraft: {},
    _pluginInv: opts.pluginInv !== undefined ? opts.pluginInv : null,
    oauthBusy: {},
    $watch: opts.$watch || null,

    cloneValue,
    unwrapType,
    defaultForType,
    mergeSubmoduleValue,
    document,

    optUi(name) {
      return this.optionsByName[name]?.ui || null;
    },
    optType(name) {
      return this.optionsByName[name]?.type || null;
    },
    hasWidget(name, widget) {
      return this.optUi(name)?.widget === widget;
    },
    deepEqual(a, b) {
      try { return JSON.stringify(a) === JSON.stringify(b); } catch (_) { return false; }
    },
    extractKeyFromItem,
    deriveKeysFrom(keysFrom) {
      if (!keysFrom || !keysFrom.option) return [];
      const src = this.values[keysFrom.option];
      const extract = keysFrom.extract || 'identity';
      if (Array.isArray(src)) {
        return src
          .map((item) => extractKeyFromItem(item, extract))
          .filter((n) => n.length > 0);
      }
      if (src && typeof src === 'object' && !Array.isArray(src)) {
        return Object.keys(src);
      }
      return [];
    },
    syncKeysFromOption(optionName) {
      const opt = this.optionsByName[optionName];
      const kf = opt?.ui?.keysFrom;
      if (!kf) return;
      const names = this.deriveKeysFrom(kf);
      const prev = (this.values[optionName] && typeof this.values[optionName] === 'object'
        && !Array.isArray(this.values[optionName]))
        ? this.values[optionName]
        : {};
      const elem = unwrapType(opt.type?.elem);
      const next = {};
      names.forEach((n) => {
        if (Object.prototype.hasOwnProperty.call(prev, n)) {
          next[n] = prev[n];
        } else {
          next[n] = defaultForType(elem || { kind: 'submodule' });
        }
      });
      this.values[optionName] = next;
      const w = NeoWidgets && NeoWidgets.get(opt.ui?.widget);
      if (w && typeof w.onKeysFromSync === 'function') {
        w.onKeysFromSync.call(this, optionName);
      }
    },
    notifyKeysFromSource(sourceName) {
      Object.keys(this.optionsByName || {}).forEach((name) => {
        const kf = this.optionsByName[name]?.ui?.keysFrom;
        if (kf && kf.option === sourceName) {
          this.syncKeysFromOption(name);
        }
      });
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
    toggleNestedListChoice(parentName, key, field, choice, checked) {
      const obj = this.ensureAttrs(parentName);
      const entry = Object.assign({}, obj[key] || {});
      const list = Array.isArray(entry[field]) ? [...entry[field]] : [];
      const i = list.indexOf(choice);
      if (checked && i < 0) list.push(choice);
      if (!checked && i >= 0) list.splice(i, 1);
      entry[field] = list;
      obj[key] = entry;
      this.values[parentName] = { ...obj };
    },
    addListItem(name) {
      const list = this.ensureList(name);
      const elem = unwrapType(this.optType(name)?.elem);
      list.push(defaultForType(elem || { kind: 'str' }));
      this.values[name] = [...list];
      this.notifyKeysFromSource(name);
    },
    removeListItem(name, idx) {
      const list = this.ensureList(name);
      list.splice(idx, 1);
      this.values[name] = [...list];
      this.notifyKeysFromSource(name);
    },
    isAtOriginal(name) {
      if (!name) return true;
      return this.deepEqual(this.values[name], this.originals[name]);
    },
    isAtDefault(name) {
      if (!name) return true;
      const w = NeoWidgets && NeoWidgets.get(this.optUi(name)?.widget);
      if (w && typeof w.isAtDefault === 'function') {
        return w.isAtDefault.call(this, name);
      }
      return this.deepEqual(this.values[name], this.defaults[name]);
    },
    initWidgets() {
      Object.keys(this.optionsByName || {}).forEach((name) => {
        const w = NeoWidgets && NeoWidgets.get(this.optUi(name)?.widget);
        if (w && typeof w.init === 'function') w.init.call(this, name);
      });
    },
    resetField(name) {
      if (!name) return;
      const opt = this.optionsByName[name];
      if (opt && opt.default !== undefined) {
        this.values[name] = cloneValue(this.defaults[name]);
      }
      const w = NeoWidgets && NeoWidgets.get(this.optUi(name)?.widget);
      if (w && typeof w.onReset === 'function') w.onReset.call(this, name);
    },
    revertField(name) {
      if (!name) return;
      if (!(name in this.originals)) return;
      this.values[name] = cloneValue(this.originals[name]);
      const w = NeoWidgets && NeoWidgets.get(this.optUi(name)?.widget);
      if (w && typeof w.onRevert === 'function') w.onRevert.call(this, name);
    },
    collectSave() {
      const toSave = {};
      Object.keys(this.values || {}).forEach((k) => {
        const w = NeoWidgets && NeoWidgets.get(this.optUi(k)?.widget);
        if (w && typeof w.prepareSave === 'function') {
          const prepared = w.prepareSave.call(this, k);
          if (prepared !== undefined) toSave[k] = prepared;
          return;
        }
        if (!this.isAtDefault(k)) {
          toSave[k] = this.values[k];
        }
      });
      return toSave;
    },
  };

  (opts.options || []).forEach((o) => {
    host.optionsByName[o.name] = o;
    if (host.values[o.name] === undefined) {
      host.values[o.name] = cloneValue(o.current !== undefined && o.current !== null ? o.current : o.default);
    }
    if (host.defaults[o.name] === undefined) {
      host.defaults[o.name] = cloneValue(o.default);
    }
    if (host.originals[o.name] === undefined) {
      host.originals[o.name] = cloneValue(host.values[o.name]);
    }
    host.hadCurrent[o.name] = (o.current !== undefined && o.current !== null);
  });

  if (NeoWidgets) Object.assign(host, NeoWidgets.mixins());
  return host;
}

module.exports = {
  cloneValue,
  createDocument,
  createFakeElement,
  installGlobals,
  makeForm,
};
