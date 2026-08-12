// option_form.js
// Alpine form controller for service options + HTMX/Alpine re-init after swaps.
// Supports scalar fields, listOf/attrsOf of scalars, one-deep submodule collections,
// and declarative ui.widget handlers (see nix/lib/ui.nix).

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
    saveBusy: false,
    saveFlash: '', // '' | 'ok' | 'err'
    saveError: '',
    /**
     * Ephemeral UI state per option (not saved).
     * exclusiveListPair: { modes: { [entryKey]: modeId } }
     */
    uiState: {},

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

      // exclusiveListPair: keep mode map aligned
      if (opt.ui?.widget === 'exclusiveListPair') {
        this.elpSyncModes(optionName);
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

    // ── exclusiveListPair widget ─────────────────────────────────────

    elpModes(optionName) {
      return this.optUi(optionName)?.modes || [];
    },

    elpModeDef(optionName, modeId) {
      return this.elpModes(optionName).find((m) => m.id === modeId) || null;
    },

    elpListFieldNames(optionName) {
      const names = new Set();
      this.elpModes(optionName).forEach((m) => {
        (m.active || []).forEach((f) => names.add(f));
      });
      return [...names];
    },

    elpEnsureState(optionName) {
      if (!this.uiState[optionName]) {
        this.uiState[optionName] = { modes: {} };
      }
      if (!this.uiState[optionName].modes) {
        this.uiState[optionName].modes = {};
      }
      return this.uiState[optionName];
    },

    /** Infer mode from data (first mode with non-empty active lists, else open/empty active). */
    elpInferMode(optionName, entry) {
      const modes = this.elpModes(optionName);
      const e = entry || {};
      for (let i = 0; i < modes.length; i++) {
        const m = modes[i];
        const active = m.active || [];
        if (active.length === 0) continue;
        const has = active.some((f) => Array.isArray(e[f]) && e[f].length > 0);
        if (has) return m.id;
      }
      // Prefer mode with empty active (open)
      const open = modes.find((m) => !(m.active || []).length);
      return open ? open.id : (modes[0]?.id || 'open');
    },

    elpSyncModes(optionName) {
      const st = this.elpEnsureState(optionName);
      const prevModes = st.modes || {};
      const nextModes = {};
      const map = this.values[optionName] || {};
      Object.keys(map).forEach((key) => {
        const e = map[key] || {};
        const anyList = this.elpListFieldNames(optionName).some(
          (f) => Array.isArray(e[f]) && e[f].length > 0
        );
        // Data with picks wins; otherwise keep sticky UI mode (empty allow/block still needs a mode).
        if (anyList) {
          nextModes[key] = this.elpInferMode(optionName, e);
        } else if (prevModes[key] && this.elpModeDef(optionName, prevModes[key])) {
          nextModes[key] = prevModes[key];
        } else {
          nextModes[key] = this.elpInferMode(optionName, e);
        }
      });
      this.uiState = { ...this.uiState, [optionName]: { ...st, modes: nextModes } };
    },

    elpMode(optionName, key) {
      const st = this.uiState[optionName];
      const ui = st?.modes?.[key];
      if (ui && this.elpModeDef(optionName, ui)) return ui;
      const e = (this.values[optionName] || {})[key] || {};
      return this.elpInferMode(optionName, e);
    },

    elpModeLabel(optionName, key) {
      const m = this.elpModeDef(optionName, this.elpMode(optionName, key));
      return m?.label || this.elpMode(optionName, key);
    },

    elpModeHint(optionName, key) {
      const modeId = this.elpMode(optionName, key);
      const m = this.elpModeDef(optionName, modeId);
      if (!m) return '';
      const e = (this.values[optionName] || {})[key] || {};
      const active = m.active || [];
      let n = 0;
      if (active.length) {
        const list = e[active[0]];
        n = Array.isArray(list) ? list.length : 0;
      }
      if (n > 0) return m.hintFilled || m.hintEmpty || '';
      return m.hintEmpty || m.hintFilled || '';
    },

    elpBadgeClass(optionName, key) {
      const m = this.elpModeDef(optionName, this.elpMode(optionName, key));
      const b = m?.badge || '';
      if (b === 'success') return 'badge-success badge-outline';
      if (b === 'primary') return 'badge-primary badge-outline';
      if (b === 'warning') return 'badge-warning badge-outline';
      if (b === 'error') return 'badge-error badge-outline';
      return 'badge-ghost';
    },

    elpListLabel(optionName, key) {
      const m = this.elpModeDef(optionName, this.elpMode(optionName, key));
      return m?.listLabel || m?.label || 'Items';
    },

    setElpMode(optionName, key, modeId) {
      const st = this.elpEnsureState(optionName);
      st.modes = { ...st.modes, [key]: modeId };
      this.uiState = { ...this.uiState, [optionName]: { ...st } };

      const mode = this.elpModeDef(optionName, modeId);
      const active = mode?.active || [];
      const allLists = this.elpListFieldNames(optionName);
      const obj = this.ensureAttrs(optionName);
      const prev = Object.assign({}, obj[key] || {});
      // Collect previous picks from any list field (for mode switch carry-over)
      let carried = [];
      allLists.forEach((f) => {
        if (Array.isArray(prev[f]) && prev[f].length) carried = [...prev[f]];
      });
      const next = Object.assign({}, prev);
      allLists.forEach((f) => { next[f] = []; });
      if (active.length === 1) {
        const field = active[0];
        const prevField = Array.isArray(prev[field]) ? prev[field] : [];
        next[field] = prevField.length ? [...prevField] : [...carried];
      }
      obj[key] = next;
      this.values[optionName] = { ...obj };
    },

    /** Choices from first nested field that has type.values (from ui.choices). */
    elpChoices(optionName) {
      const fields = this.optType(optionName)?.elem?.fields || [];
      for (let i = 0; i < fields.length; i++) {
        const vals = fields[i]?.type?.values;
        if (Array.isArray(vals) && vals.length) return vals;
      }
      // Prefer empty array over undefined for x-for
      for (let i = 0; i < fields.length; i++) {
        const vals = fields[i]?.type?.values;
        if (Array.isArray(vals)) return vals;
      }
      return [];
    },

    elpChoiceEmptyHint(optionName) {
      return this.optUi(optionName)?.choiceEmptyHint || 'No choices available.';
    },

    elpEmptyHint(optionName) {
      return this.optUi(optionName)?.emptyHint || '';
    },

    elpEntryLabel(optionName) {
      return this.optUi(optionName)?.entryLabel || 'Entry';
    },

    elpAppSelected(optionName, key, app) {
      const modeId = this.elpMode(optionName, key);
      const mode = this.elpModeDef(optionName, modeId);
      const active = mode?.active || [];
      if (!active.length) return false;
      const e = (this.values[optionName] || {})[key] || {};
      const list = e[active[0]];
      return Array.isArray(list) && list.includes(app);
    },

    toggleElpApp(optionName, key, app, checked) {
      const modeId = this.elpMode(optionName, key);
      const mode = this.elpModeDef(optionName, modeId);
      const active = mode?.active || [];
      if (!active.length) return;
      // Sticky mode while picking
      const st = this.elpEnsureState(optionName);
      st.modes = { ...st.modes, [key]: modeId };
      this.uiState = { ...this.uiState, [optionName]: { ...st } };
      this.toggleNestedListChoice(optionName, key, active[0], app, checked);
    },

    setAllElpApps(optionName, key, selectAll) {
      const modeId = this.elpMode(optionName, key);
      const mode = this.elpModeDef(optionName, modeId);
      const active = mode?.active || [];
      if (!active.length) return;
      const st = this.elpEnsureState(optionName);
      st.modes = { ...st.modes, [key]: modeId };
      this.uiState = { ...this.uiState, [optionName]: { ...st } };
      const apps = this.elpChoices(optionName);
      const allLists = this.elpListFieldNames(optionName);
      const obj = this.ensureAttrs(optionName);
      const entry = Object.assign({}, obj[key] || {});
      allLists.forEach((f) => { entry[f] = []; });
      entry[active[0]] = selectAll ? [...apps] : [];
      obj[key] = entry;
      this.values[optionName] = { ...obj };
    },

    elpPruneEmptyEntries(optionName, value) {
      const lists = this.elpListFieldNames(optionName);
      const out = {};
      Object.keys(value || {}).forEach((k) => {
        const e = value[k] || {};
        const any = lists.some((f) => Array.isArray(e[f]) && e[f].length > 0);
        if (any) {
          const entry = {};
          lists.forEach((f) => {
            entry[f] = Array.isArray(e[f]) ? e[f] : [];
          });
          // Keep any non-list fields too
          Object.keys(e).forEach((fk) => {
            if (!lists.includes(fk)) entry[fk] = e[fk];
          });
          out[k] = entry;
        }
      });
      return out;
    },

    elpIsAtDefault(optionName) {
      const save = this.optUi(optionName)?.save || {};
      let v = this.values[optionName];
      if (save.pruneEmptyEntries) {
        v = this.elpPruneEmptyEntries(optionName, v);
      }
      if (save.omitIfEmpty || save.pruneEmptyEntries) {
        return Object.keys(v || {}).length === 0;
      }
      return this.deepEqual(v, this.defaults[optionName]);
    },

    elpPrepareSave(optionName) {
      const save = this.optUi(optionName)?.save || {};
      let v = this.cloneValue(this.values[optionName]);
      if (save.pruneEmptyEntries) {
        v = this.elpPruneEmptyEntries(optionName, v);
      }
      if (save.omitIfEmpty && Object.keys(v || {}).length === 0) {
        return undefined; // omit from payload
      }
      return v;
    },

    initExclusiveListPair(optionName) {
      this.syncKeysFromOption(optionName);
      this.elpSyncModes(optionName);
      this.originals[optionName] = this.cloneValue(this.values[optionName]);
      const kf = this.optUi(optionName)?.keysFrom;
      if (kf?.option && typeof this.$watch === 'function') {
        this.$watch(`values.${kf.option}`, () => {
          this.syncKeysFromOption(optionName);
        });
      }
    },

    // ── Widget lifecycle ─────────────────────────────────────────────

    initWidgets() {
      Object.keys(this.optionsByName || {}).forEach((name) => {
        const w = this.optUi(name)?.widget;
        if (w === 'exclusiveListPair') {
          this.initExclusiveListPair(name);
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

      this.initWidgets();
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
      if (this.hasWidget(name, 'exclusiveListPair')) {
        this.syncKeysFromOption(name);
      }
    },

    revertField(name) {
      if (!name) return;
      const origs = this.originals || {};
      if (!(name in origs)) return;
      this.values[name] = this.cloneValue(origs[name]);
      if (this.hasWidget(name, 'exclusiveListPair')) {
        this.elpSyncModes(name);
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
      if (this.hasWidget(name, 'exclusiveListPair')) {
        return this.elpIsAtDefault(name);
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

      const toSave = {};
      Object.keys(this.values || {}).forEach((k) => {
        if (this.hasWidget(k, 'exclusiveListPair')) {
          const prepared = this.elpPrepareSave(k);
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
          if (typeof window.neoToast === 'function') {
            window.neoToast('Settings saved', 'success');
          }
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
          if (typeof window.neoToast === 'function') {
            window.neoToast(this.saveError, 'error');
          }
        }
      } catch (e) {
        this.saveBusy = false;
        this.saveFlash = 'err';
        this.saveError = String(e);
        if (typeof window.neoToast === 'function') {
          window.neoToast(this.saveError.slice(0, 240), 'error');
        }
      }
    }
  }
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
