// option_form.js
// Alpine form controller for service options + HTMX/Alpine re-init after swaps.
// Supports scalar fields, listOf/attrsOf of scalars, one-deep submodule collections,
// and declarative ui.widget handlers (see nix/lib/ui.nix).
// Widget implementations live next to their Handlebars templates under
// templates/options/widgets/ and register on NeoWidgets.

function neoWidget(name) {
  if (typeof NeoWidgets === 'undefined' || !name) return null;
  return NeoWidgets.get(name) || null;
}

/** ui.visibleWhen "opt", "opt=value" or "opt=a,b" → [opt, value | undefined]. */
function splitVisibleWhen(rule) {
  if (!rule) return [null, undefined];
  const at = rule.indexOf('=');
  return at < 0 ? [rule, undefined] : [rule.slice(0, at), rule.slice(at + 1)];
}

function visibleFor(value, want) {
  return want === undefined ? !!value : want.split(',').includes(String(value));
}

function optionForm() {
  const host = {
    values: {},
    defaults: {},
    originals: {},
    hadCurrent: {},
    optionsByName: {},
    serviceName: '',
    isCore: false,
    helperBusy: false,
    saveBusy: false,
    saveFlash: '', // '' | 'ok' | 'err'
    saveError: '',
    /**
     * Ephemeral UI state per option (not saved). Widgets store mode maps,
     * OAuth dialogs, etc. here.
     */
    uiState: {},
    plDraft: {},
    plDraftPrefix: {},
    plError: {},
    plEdit: {},
    plPrefixAuto: {},
    _pluginInv: null,
    oauthBusy: {},
    /** Active pane tab: 'settings' | 'status' | 'snapshots'. */
    paneTab: 'settings',
    /** Collapsible section id → open (remembered per service in localStorage). */
    openSections: {},

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
        case 'strMatching':
        case 'separatedString':
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
            } else if (f.type && f.type.kind === 'nullOr') {
              // Stays null, so the save leaves it out of settings.toml.
              obj[f.name] = null;
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

      if (kind === 'submodule') {
        return this.mergeSubmoduleValue(v, type);
      }

      return this.cloneValue(v);
    },

    // ── Schema / ui helpers ──────────────────────────────────────────

    optUi(name) {
      return this.optionsByName[name]?.ui || null;
    },

    hasWidget(name, widget) {
      return this.optUi(name)?.widget === widget;
    },

    // ── keysFrom (generic) ───────────────────────────────────────────

    extractKeyFromItem(item, extract) {
      const s = String(item ?? '');
      if (extract === 'beforeColon') {
        const i = s.indexOf(':');
        return i >= 0 ? s.slice(0, i) : s;
      }
      return s;
    },

    deriveKeysFrom(keysFrom) {
      if (!keysFrom || !keysFrom.option) return [];
      const src = this.values[keysFrom.option];
      const extract = keysFrom.extract || 'identity';
      if (Array.isArray(src)) {
        return src
          .map((item) => this.extractKeyFromItem(item, extract))
          .filter((n) => n.length > 0);
      }
      if (src && typeof src === 'object' && !Array.isArray(src)) {
        return Object.keys(src);
      }
      return [];
    },

    /**
     * Align attrsOf keys with keysFrom source; seed missing entries with submodule defaults.
     */
    syncKeysFromOption(optionName) {
      const opt = this.optionsByName[optionName];
      const kf = opt?.ui?.keysFrom;
      if (!kf) return;
      const names = this.deriveKeysFrom(kf);
      const prev = (this.values[optionName] && typeof this.values[optionName] === 'object'
        && !Array.isArray(this.values[optionName]))
        ? this.values[optionName]
        : {};
      const elem = this.unwrapType(opt.type?.elem);
      const next = {};
      names.forEach((n) => {
        if (Object.prototype.hasOwnProperty.call(prev, n)) {
          next[n] = prev[n];
        } else {
          next[n] = this.defaultForType(elem || { kind: 'submodule' });
        }
      });
      this.values[optionName] = next;

      const w = neoWidget(opt.ui?.widget);
      if (w && typeof w.onKeysFromSync === 'function') {
        w.onKeysFromSync.call(this, optionName);
      }
    },

    /** Re-sync every option that keysFrom the given source option name. */
    notifyKeysFromSource(sourceName) {
      Object.keys(this.optionsByName || {}).forEach((name) => {
        const kf = this.optionsByName[name]?.ui?.keysFrom;
        if (kf && kf.option === sourceName) {
          this.syncKeysFromOption(name);
        }
      });
    },

    // ── Widget lifecycle ─────────────────────────────────────────────

    initWidgets() {
      Object.keys(this.optionsByName || {}).forEach((name) => {
        const w = neoWidget(this.optUi(name)?.widget);
        if (w && typeof w.init === 'function') {
          w.init.call(this, name);
        }
      });
    },

    initForm() {
      const raw = document.getElementById('options-seed')?.textContent || '[]';
      let opts = [];
      try { opts = JSON.parse(raw); } catch (e) { opts = []; }

      this.optionsByName = {};
      this.uiState = {};
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
      this.initPaneChrome();

      this.initWidgets();
      // Widget init may canonicalize values (null → '', keysFrom fill). That is
      // not a user edit — recapture originals so leave/dirty stays clean.
      Object.keys(this.values || {}).forEach((k) => {
        this.originals[k] = this.cloneValue(this.values[k]);
      });
    },

    /**
     * Save blockers from widgets that implement validate(optionName) → string[].
     * Widgets flag the offending rows inline; save() refuses while any remain.
     */
    widgetValidationErrors() {
      const out = [];
      Object.keys(this.optionsByName || {}).forEach((name) => {
        const w = neoWidget(this.optUi(name)?.widget);
        if (w && typeof w.validate === 'function') {
          (w.validate.call(this, name) || []).forEach((msg) => out.push(msg));
        }
      });
      return out;
    },

    /**
     * Live / save-time check for types.strMatching (and nullOr of it).
     * Nix builtins.match is whole-string; JS mirrors that with ^(?:…)$.
     * Unusable JS patterns skip client checks (activate still enforces).
     */
    patternErrorForType(type, value) {
      if (!type) return '';
      let t = type;
      let v = value;
      if (t.kind === 'nullOr') {
        if (v === null || v === undefined || v === '') return '';
        t = t.elem || {};
      }
      if (t.kind !== 'strMatching') return '';
      const pat = t.pattern;
      if (!pat) return '';
      const s = v == null ? '' : String(v);
      try {
        const re = new RegExp('^(?:' + pat + ')$');
        if (!re.test(s)) return 'Must match /' + pat + '/';
      } catch (_) {
        return '';
      }
      return '';
    },

    /** separatedString value shown one entry per line. */
    sepLines(name) {
      const v = this.values[name];
      const s = v == null ? '' : String(v);
      const sep = this.optionsByName[name]?.type?.separator ?? '\n';
      return (sep === '\n' || sep === '') ? s : s.split(sep).join('\n');
    },

    setSepLines(name, text) {
      const sep = this.optionsByName[name]?.type?.separator ?? '\n';
      this.values[name] = (sep === '\n' || sep === '') ? text : text.split('\n').join(sep);
    },

    patternError(name) {
      const opt = this.optionsByName[name];
      if (!opt) return '';
      return this.patternErrorForType(opt.type, this.values[name]);
    },

    /** Top-level scalar pattern failures (strMatching). */
    scalarValidationErrors() {
      const out = [];
      Object.keys(this.optionsByName || {}).forEach((name) => {
        if (typeof this.isFieldVisible === 'function' && !this.isFieldVisible(name)) return;
        const msg = this.patternError(name);
        if (msg) {
          const label = this.optionsByName[name]?.label || name;
          out.push(label + ': ' + msg);
        }
      });
      return out;
    },

    // ── Pane chrome: tabs, sections, visibility, summaries ───────────

    _prefKey(kind) {
      return 'neo.pane.' + kind + '.' + (this.isCore ? 'core.' : '') + (this.serviceName || '');
    },

    _loadPref(kind, fallback) {
      return window.NeoUtil.store('local').getJson(this._prefKey(kind), fallback);
    },

    _savePref(kind, value) {
      window.NeoUtil.store('local').setJson(this._prefKey(kind), value);
    },

    initPaneChrome() {
      const pane = document.getElementById('options-pane');
      const has = (sel) => typeof pane?.querySelector === 'function' && !!pane.querySelector(sel);
      const tabs = ['settings'];
      if (has('#runtime-units')) tabs.push('status');
      if (has('[id^="snapshots-"]')) tabs.push('snapshots');
      const tab = this._loadPref('tab', 'settings');
      this.paneTab = tabs.includes(tab) ? tab : 'settings';
      const open = this._loadPref('sections', {});
      this.openSections = (open && typeof open === 'object') ? open : {};
    },

    setPaneTab(tab) {
      this.paneTab = tab;
      this._savePref('tab', tab);
    },

    isSectionOpen(id) {
      return !!this.openSections[id];
    },

    toggleSection(id) {
      this.openSections = { ...this.openSections, [id]: !this.openSections[id] };
      this._savePref('sections', this.openSections);
    },

    expandAllSections(open) {
      const next = {};
      document.querySelectorAll('#options-pane section[data-section]').forEach((el) => {
        next[el.dataset.section] = !!open;
      });
      this.openSections = next;
      this._savePref('sections', next);
      this.setPaneTab('settings');
    },

    isDirty() {
      return Object.keys(this.originals || {}).some((k) => !this.isAtOriginal(k));
    },

    dirtyCount() {
      return Object.keys(this.originals || {}).filter((k) => !this.isAtOriginal(k)).length;
    },

    customizedCount(names) {
      return (names || []).filter((n) => n in (this.defaults || {}) && !this.isAtDefault(n)).length;
    },

    /**
     * ui.visibleWhen (resolved to an absolute name in Rust): "opt" hides while that
     * bool is false, "opt=value" hides unless it equals value ("opt=a,b": one of them).
     */
    isFieldVisible(name) {
      const [dep, want] = splitVisibleWhen(this.optionsByName[name]?.visibleWhen);
      if (!dep || !(dep in (this.values || {}))) return true;
      return visibleFor(this.values[dep], want);
    },

    /** Same rule for a field of one attrsOf submodule entry (sibling = last path segment). */
    isEntryFieldVisible(name, key, rule) {
      const [dep, want] = splitVisibleWhen(rule);
      if (!dep) return true;
      const entry = (this.values || {})[name]?.[key];
      const sib = dep.split('.').pop();
      if (!entry || !(sib in entry)) return true;
      return visibleFor(entry[sib], want);
    },

    choiceAt(name, idx) {
      const t = this.optionsByName[name]?.type || {};
      const list = t.choices || (t.elem && t.elem.choices) || [];
      return list[idx];
    },

    choiceLabel(name, value) {
      const t = this.optionsByName[name]?.type || {};
      const list = t.choices || (t.elem && t.elem.choices) || [];
      const hit = list.find((c) => c.value === value || String(c.value) === String(value));
      return hit ? hit.label : String(value);
    },

    isChoiceSelected(name, idx) {
      const c = this.choiceAt(name, idx);
      return !!c && String(this.values[name]) === String(c.value);
    },

    selectChoice(name, idx) {
      const c = this.choiceAt(name, idx);
      if (c) this.values[name] = this.cloneValue(c.value);
    },

    isListChoiceSelected(name, idx) {
      const c = this.choiceAt(name, idx);
      return !!c && (this.values[name] || []).includes(c.value);
    },

    toggleListChoiceAt(name, idx) {
      const c = this.choiceAt(name, idx);
      if (!c) return;
      this.toggleListChoice(name, c.value, !this.isListChoiceSelected(name, idx));
    },

    /** Chip for a ui.summary option on a collapsed section header. */
    summaryChip(name) {
      const opt = this.optionsByName[name];
      if (!opt) return { text: '', cls: 'hidden' };
      const label = opt.label || name;
      const v = this.values[name];
      const on = 'badge-soft badge-primary';
      const off = 'badge-ghost text-base-content/60';
      if (typeof v === 'boolean') {
        return { text: (v ? '✓ ' : '✕ ') + label, cls: v ? on : off };
      }
      if (Array.isArray(v)) {
        if (!v.length) return { text: label + ': none', cls: off };
        const shown = v.slice(0, 3).map((x) => this.choiceLabel(name, x)).join(', ');
        return { text: label + ': ' + shown + (v.length > 3 ? ' +' + (v.length - 3) : ''), cls: on };
      }
      if (v === null || v === undefined || v === '') return { text: label + ': not set', cls: off };
      if (typeof v === 'object') return { text: label + ': ' + Object.keys(v).length, cls: on };
      return { text: label + ': ' + this.choiceLabel(name, v), cls: on };
    },

    toggleListChoice(optionName, choice, checked) {
      const list = Array.isArray(this.values[optionName]) ? [...this.values[optionName]] : [];
      const i = list.indexOf(choice);
      if (checked && i < 0) list.push(choice);
      if (!checked && i >= 0) list.splice(i, 1);
      this.values[optionName] = list;
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
        this.notifyKeysFromSource(optionName);
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
        this.notifyKeysFromSource(optionName);
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
      const w = neoWidget(this.optUi(name)?.widget);
      if (w && typeof w.onReset === 'function') {
        w.onReset.call(this, name);
      }
    },

    revertField(name) {
      if (!name) return;
      const origs = this.originals || {};
      if (!(name in origs)) return;
      this.values[name] = this.cloneValue(origs[name]);
      const w = neoWidget(this.optUi(name)?.widget);
      if (w && typeof w.onRevert === 'function') {
        w.onRevert.call(this, name);
      }
    },

    resetAll() {
      Object.keys(this.defaults).forEach((k) => this.resetField(k));
    },

    deepEqual(a, b) {
      try { return JSON.stringify(a) === JSON.stringify(b); } catch (_) { return false; }
    },

    isAtDefault(name) {
      if (!name) return true;
      const w = neoWidget(this.optUi(name)?.widget);
      if (w && typeof w.isAtDefault === 'function') {
        return w.isAtDefault.call(this, name);
      }
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
      this.notifyKeysFromSource(name);
    },

    removeListItem(name, idx) {
      const list = this.ensureList(name);
      list.splice(idx, 1);
      this.values[name] = [...list];
      this.notifyKeysFromSource(name);
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

    revertAll() {
      const origs = this.originals || {};
      Object.keys(origs).forEach((k) => this.revertField(k));
    },

    async save() {
      if (this.saveBusy) return;
      const svc = this.serviceName || 'service';
      const pane = document.getElementById('options-pane');
      const ep = (pane && pane.dataset && pane.dataset.saveEndpoint) || `/save/${encodeURIComponent(svc)}`;

      // keysFrom: align before write
      Object.keys(this.optionsByName || {}).forEach((name) => {
        if (this.optUi(name)?.keysFrom) {
          this.syncKeysFromOption(name);
        }
      });

      const invalid = this.widgetValidationErrors().concat(this.scalarValidationErrors());
      if (invalid.length) {
        this.saveFlash = 'err';
        this.saveError = invalid[0] + (invalid.length > 1 ? ` (+${invalid.length - 1} more)` : '');
        window.neoToast(('Fix before saving — ' + this.saveError).slice(0, 240), 'error');
        return;
      }

      const toSave = {};
      Object.keys(this.values || {}).forEach((k) => {
        const w = neoWidget(this.optUi(k)?.widget);
        if (w && typeof w.prepareSave === 'function') {
          const prepared = w.prepareSave.call(this, k);
          if (prepared !== undefined) {
            toSave[k] = prepared;
          }
          return;
        }
        if (!this.isAtDefault(k)) {
          toSave[k] = this.values[k];
        }
      });

      this.saveBusy = true;
      this.saveFlash = '';
      this.saveError = '';
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
          this.saveBusy = false;
          this.saveFlash = 'ok';
          window.neoToast('Settings saved', 'success');
          // Keep success feedback visible before the pane reload (nix re-eval).
          await new Promise((r) => setTimeout(r, 900));
          const loadUrl = pane?.dataset?.loadUrl;
          if (loadUrl) {
            htmx.ajax('GET', loadUrl, {
              target: '#config-content',
              swap: 'innerHTML',
            });
          } else {
            setTimeout(() => { if (this.saveFlash === 'ok') this.saveFlash = ''; }, 1500);
          }
        } else {
          const txt = await res.text().catch(() => '');
          this.saveBusy = false;
          this.saveFlash = 'err';
          this.saveError = (txt || ('HTTP ' + res.status)).slice(0, 240);
          window.neoToast(this.saveError, 'error');
        }
      } catch (e) {
        this.saveBusy = false;
        this.saveFlash = 'err';
        this.saveError = String(e);
        window.neoToast(this.saveError.slice(0, 240), 'error');
      }
    }
  };

  if (typeof NeoWidgets !== 'undefined') {
    Object.assign(host, NeoWidgets.mixins());
  }
  return host;
}

// Ensure Alpine picks up x-data etc. after HTMX swaps (options pane + services grid).
document.addEventListener('htmx:afterSettle', () => {
  if (typeof Alpine === 'undefined') return;
  const pane = document.getElementById('options-pane');
  if (pane && pane.hasAttribute('x-data')) {
    Alpine.initTree(pane);
  }
  const grid = document.getElementById('services-grid');
  if (grid && grid.hasAttribute('x-data')) {
    Alpine.initTree(grid);
  }
});
