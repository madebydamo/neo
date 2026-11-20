// option_form_core.js — the option form's logic without DOM or form state:
// type defaults, value normalization, keysFrom keys, strMatching checks,
// separatedString lines, visibleWhen rules, choice labels, summary chips and
// entry-default pruning. option_form.js (loaded after this file) delegates to
// it; nothing here knows a service or option name. Tested by
// option_form_core.test.js.
(function (root, factory) {
  var api = factory();
  if (root) root.NeoOptionFormCore = api;
  if (typeof module === 'object' && module.exports) module.exports = api;
})(typeof window !== 'undefined' ? window : typeof globalThis !== 'undefined' ? globalThis : null, function () {
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

  function deepEqual(a, b) {
    try { return JSON.stringify(a) === JSON.stringify(b); } catch (_) { return false; }
  }

  function isPlainObject(v) {
    return !!v && typeof v === 'object' && !Array.isArray(v);
  }

  // ── Types and values ──────────────────────────────────────────────

  function unwrapType(type) {
    if (!type) return type;
    if (type.kind === 'nullOr' && type.elem) return type.elem;
    return type;
  }

  /** The empty value of a type (submodule: its fields' defaults). */
  function defaultForType(type) {
    const t = unwrapType(type);
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
            obj[f.name] = cloneValue(f.default);
          } else if (f.type && f.type.kind === 'nullOr') {
            // Stays null, so the save leaves it out of settings.toml.
            obj[f.name] = null;
          } else {
            obj[f.name] = defaultForType(f.type);
          }
        });
        return obj;
      }
      default:
        return null;
    }
  }

  // ── Nested submodules inside an entry ─────────────────────────────
  // A plain submodule inside an entry comes from Nix as dotted field names
  // (`postgres.password`). settings.toml and the evaluation hold it nested
  // (`postgres = { password = … }`); the form edits it under the flat name.
  // Saved flat, the key would be the literal `"postgres.password"`: a setting
  // Nix does not know, and a secret the secrets split does not find.

  /** Fields of a submodule type whose name is a dotted path. */
  function dottedFields(type) {
    const t = unwrapType(type);
    if (!t || t.kind !== 'submodule') return [];
    return (t.fields || []).filter((f) => f && typeof f.name === 'string' && f.name.includes('.'));
  }

  /** Remove the leaf at `parts` from `obj` and the tables it leaves empty. */
  function takePath(obj, parts) {
    const [head, ...rest] = parts;
    if (!isPlainObject(obj) || !(head in obj)) return undefined;
    if (!rest.length) {
      const v = obj[head];
      delete obj[head];
      return v;
    }
    const v = takePath(obj[head], rest);
    if (isPlainObject(obj[head]) && !Object.keys(obj[head]).length) delete obj[head];
    return v;
  }

  /** An entry with nested values of dotted fields moved to their flat name (load). */
  function flattenEntry(val, elemType) {
    if (!isPlainObject(val)) return val;
    const out = cloneValue(val);
    dottedFields(elemType).forEach((f) => {
      const nested = takePath(out, f.name.split('.'));
      if (out[f.name] === undefined && nested !== undefined) out[f.name] = nested;
    });
    return out;
  }

  /** An entry with flat dotted field names moved into nested tables (save). */
  function nestEntry(val, elemType) {
    if (!isPlainObject(val)) return val;
    const out = cloneValue(val);
    dottedFields(elemType).forEach((f) => {
      if (!Object.prototype.hasOwnProperty.call(out, f.name)) return;
      const v = out[f.name];
      delete out[f.name];
      if (v === null || v === undefined) return;
      const parts = f.name.split('.');
      let cur = out;
      parts.slice(0, -1).forEach((p) => {
        if (!isPlainObject(cur[p])) cur[p] = {};
        cur = cur[p];
      });
      cur[parts[parts.length - 1]] = v;
    });
    return out;
  }

  /** An option's form value as settings.toml nests it: [`nestEntry`] on each submodule entry. */
  function nestValue(type, value) {
    const t = unwrapType(type);
    if (!t) return value;
    const elem = unwrapType(t.elem);
    if (t.kind === 'listOf' && elem && elem.kind === 'submodule' && Array.isArray(value)) {
      return value.map((e) => nestEntry(e, elem));
    }
    if (t.kind === 'attrsOf' && elem && elem.kind === 'submodule' && isPlainObject(value)) {
      const out = {};
      Object.keys(value).forEach((k) => { out[k] = nestEntry(value[k], elem); });
      return out;
    }
    if (t.kind === 'submodule') return nestEntry(value, t);
    return value;
  }

  /** A submodule value with every field it lacks filled from the type. */
  function mergeSubmoduleValue(val, elemType) {
    const base = defaultForType(elemType) || {};
    const v = isPlainObject(val) ? flattenEntry(val, elemType) : {};
    Object.keys(base).forEach((k) => {
      if (v[k] === undefined) v[k] = base[k];
    });
    return v;
  }

  /** The form value of option `opt` from a saved/default value (a copy). */
  function normalizeValue(opt, raw) {
    const type = opt.type || {};
    const kind = type.kind;
    let v = raw;
    if (v === undefined) v = null;

    if (kind === 'listOf') {
      if (!Array.isArray(v)) v = Array.isArray(opt.default) ? cloneValue(opt.default) : [];
      if (type.elem && type.elem.kind === 'submodule') {
        v = v.map((item) => mergeSubmoduleValue(item, type.elem));
      } else {
        v = cloneValue(v);
      }
      return v;
    }

    if (kind === 'attrsOf') {
      if (!isPlainObject(v)) {
        v = isPlainObject(opt.default) ? cloneValue(opt.default) : {};
      } else {
        v = cloneValue(v);
      }
      if (type.elem && type.elem.kind === 'submodule') {
        const out = {};
        Object.keys(v).forEach((k) => {
          out[k] = mergeSubmoduleValue(v[k], type.elem);
        });
        return out;
      }
      return v;
    }

    if (kind === 'submodule') {
      return mergeSubmoduleValue(v, type);
    }

    return cloneValue(v);
  }

  // ── keysFrom ──────────────────────────────────────────────────────

  function extractKeyFromItem(item, extract) {
    const s = String(item ?? '');
    if (extract === 'beforeColon') {
      const i = s.indexOf(':');
      return i >= 0 ? s.slice(0, i) : s;
    }
    return s;
  }

  /** Keys an attrsOf takes from its keysFrom source value (list items or attr names). */
  function deriveKeys(keysFrom, src) {
    if (!keysFrom || !keysFrom.option) return [];
    const extract = keysFrom.extract || 'identity';
    if (Array.isArray(src)) {
      return src
        .map((item) => extractKeyFromItem(item, extract))
        .filter((n) => n.length > 0);
    }
    if (isPlainObject(src)) return Object.keys(src);
    return [];
  }

  /**
   * The attrsOf value with exactly `names` as keys, in that order: existing
   * entries kept, missing ones the default of `elemType` (a submodule when unknown).
   */
  function syncKeys(names, prev, elemType) {
    const old = isPlainObject(prev) ? prev : {};
    const next = {};
    names.forEach((n) => {
      next[n] = Object.prototype.hasOwnProperty.call(old, n)
        ? old[n]
        : defaultForType(elemType || { kind: 'submodule' });
    });
    return next;
  }

  // ── Checks and text forms ─────────────────────────────────────────

  /**
   * Live / save-time check for types.strMatching (and nullOr of it).
   * Nix builtins.match is whole-string; JS mirrors that with ^(?:…)$.
   * Unusable JS patterns skip client checks (activate still enforces).
   */
  function patternErrorForType(type, value) {
    if (!type) return '';
    let t = type;
    const v = value;
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
  }

  /** separatedString value → one entry per line. */
  function sepToLines(value, sep) {
    const s = value == null ? '' : String(value);
    const sp = sep ?? '\n';
    return (sp === '\n' || sp === '') ? s : s.split(sp).join('\n');
  }

  /** One entry per line → separatedString value. */
  function linesToSep(text, sep) {
    const sp = sep ?? '\n';
    return (sp === '\n' || sp === '') ? text : text.split('\n').join(sp);
  }

  /** The text shown when a save is refused: the first problem and how many more. */
  function invalidSummary(invalid) {
    return invalid[0] + (invalid.length > 1 ? ` (+${invalid.length - 1} more)` : '');
  }

  // ── visibleWhen ───────────────────────────────────────────────────

  /** ui.visibleWhen "opt", "opt=value" or "opt=a,b" → [opt, value | undefined]. */
  function splitVisibleWhen(rule) {
    if (!rule) return [null, undefined];
    const at = rule.indexOf('=');
    return at < 0 ? [rule, undefined] : [rule.slice(0, at), rule.slice(at + 1)];
  }

  function visibleFor(value, want) {
    return want === undefined ? !!value : want.split(',').includes(String(value));
  }

  /**
   * ui.visibleWhen (resolved to an absolute name in Rust): "opt" hides while that
   * bool is false, "opt=value" hides unless it equals value ("opt=a,b": one of them).
   * An unknown dependency never hides.
   */
  function fieldVisible(rule, values) {
    const [dep, want] = splitVisibleWhen(rule);
    if (!dep || !(dep in (values || {}))) return true;
    return visibleFor(values[dep], want);
  }

  /** Same rule for a field of one submodule entry (sibling = last path segment). */
  function entryFieldVisible(rule, entry) {
    const [dep, want] = splitVisibleWhen(rule);
    if (!dep) return true;
    const sib = dep.split('.').pop();
    if (!entry || !(sib in entry)) return true;
    return visibleFor(entry[sib], want);
  }

  // ── Choices and summaries ─────────────────────────────────────────

  /** Choice rows ({ value, label }) of an option type or its list element. */
  function choicesOf(type) {
    const t = type || {};
    return t.choices || (t.elem && t.elem.choices) || [];
  }

  function choiceLabel(type, value) {
    const hit = choicesOf(type).find((c) => c.value === value || String(c.value) === String(value));
    return hit ? hit.label : String(value);
  }

  /** `list` with `choice` added or removed (a new array; never a duplicate). */
  function toggleInList(list, choice, checked) {
    const out = Array.isArray(list) ? [...list] : [];
    const i = out.indexOf(choice);
    if (checked && i < 0) out.push(choice);
    if (!checked && i >= 0) out.splice(i, 1);
    return out;
  }

  /** Chip for a ui.summary option on a collapsed section header. */
  function summaryChip(label, v, labelOf) {
    const on = 'badge-soft badge-primary';
    const off = 'badge-ghost text-base-content/60';
    if (typeof v === 'boolean') {
      return { text: (v ? '✓ ' : '✕ ') + label, cls: v ? on : off };
    }
    if (Array.isArray(v)) {
      if (!v.length) return { text: label + ': none', cls: off };
      const shown = v.slice(0, 3).map(labelOf).join(', ');
      return { text: label + ': ' + shown + (v.length > 3 ? ' +' + (v.length - 3) : ''), cls: on };
    }
    if (v === null || v === undefined || v === '') return { text: label + ': not set', cls: off };
    if (typeof v === 'object') return { text: label + ': ' + Object.keys(v).length, cls: on };
    return { text: label + ': ' + labelOf(v), cls: on };
  }

  // ── Save ──────────────────────────────────────────────────────────

  /**
   * attrsOf of submodules with `entryDefaults` (each entry's own field defaults, from Nix):
   * drop every field that still equals its entry's default. A value a module computes per entry
   * (say an app's schedule) then stays out of settings.toml, and later changes to it still apply.
   * Entries are kept (possibly empty), so an entry the operator added is never lost.
   */
  function pruneEntryDefaults(entryDefaults, value) {
    const ed = entryDefaults;
    if (!ed || !isPlainObject(value)) return value;
    const out = {};
    Object.keys(value).forEach((k) => {
      const entry = value[k];
      const defs = ed[k];
      if (!defs || !isPlainObject(entry)) {
        out[k] = entry;
        return;
      }
      const kept = {};
      Object.keys(entry).forEach((f) => {
        if (!Object.prototype.hasOwnProperty.call(defs, f) || !deepEqual(entry[f], defs[f])) {
          kept[f] = entry[f];
        }
      });
      out[k] = kept;
    });
    return out;
  }

  return {
    cloneValue,
    deepEqual,
    unwrapType,
    defaultForType,
    mergeSubmoduleValue,
    flattenEntry,
    nestEntry,
    nestValue,
    normalizeValue,
    extractKeyFromItem,
    deriveKeys,
    syncKeys,
    patternErrorForType,
    sepToLines,
    linesToSep,
    invalidSummary,
    splitVisibleWhen,
    visibleFor,
    fieldVisible,
    entryFieldVisible,
    choicesOf,
    choiceLabel,
    toggleInList,
    summaryChip,
    pruneEntryDefaults,
  };
});
