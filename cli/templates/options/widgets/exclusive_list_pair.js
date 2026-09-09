// exclusiveListPair — attrsOf submodule with exclusive list fields + open mode.
// Load after registry.js. Mixins keep elp* names used by exclusive_list_pair.html.hbs.
(function (root) {
  if (!root.NeoWidgets) {
    throw new Error('registry.js must load before exclusive_list_pair.js');
  }

  const mixins = {
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
  };

  const widget = {
    name: 'exclusiveListPair',
    mixins,

    init(optionName) {
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

    prepareSave(optionName) {
      return this.elpPrepareSave(optionName);
    },

    isAtDefault(optionName) {
      return this.elpIsAtDefault(optionName);
    },

    onReset(optionName) {
      this.syncKeysFromOption(optionName);
    },

    onRevert(optionName) {
      this.elpSyncModes(optionName);
    },

    onKeysFromSync(optionName) {
      this.elpSyncModes(optionName);
    },
  };

  root.NeoWidgets.register(widget.name, widget);
  if (typeof module === 'object' && module.exports) module.exports = widget;
})(typeof globalThis !== 'undefined' ? globalThis : this);
