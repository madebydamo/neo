// pluginList widget — core.plugins instances keyed by id: { <id>: { url, prefix } }.
// Add (with an optional prefix), edit the address or branch in place, remove with
// an uninstall confirm. Rules mirror cli/src/utils/plugins.rs (ids, prefixes,
// same plugin twice) — see docs/design/plugin-instances.md.
(function (root) {
  if (!root.NeoWidgets) {
    throw new Error('registry.js must load before plugin_list.js');
  }

  const FORGES = ['github:', 'gitlab:', 'sourcehut:'];

  function splitQuery(url) {
    let s = String(url || '').trim();
    let frag = '';
    const h = s.indexOf('#');
    if (h >= 0) { frag = s.slice(h); s = s.slice(0, h); }
    let query = '';
    const q = s.indexOf('?');
    if (q >= 0) { query = s.slice(q + 1); s = s.slice(0, q); }
    const params = query ? query.split('&').filter(Boolean) : [];
    return { base: s, params, frag };
  }

  function joinQuery(base, params, frag) {
    return base + (params.length ? '?' + params.join('&') : '') + (frag || '');
  }

  const mixins = {
    pluginInventory() {
      if (this._pluginInv) return this._pluginInv;
      try {
        const raw = document.getElementById('plugin-inventory-seed')?.textContent || '[]';
        this._pluginInv = JSON.parse(raw);
      } catch (_) {
        this._pluginInv = [];
      }
      if (!Array.isArray(this._pluginInv)) this._pluginInv = [];
      return this._pluginInv;
    },

    // Repository name (forge refs) or last path segment. Not an identity.
    pluginLabel(url) {
      const trimmed = String(url || '').trim();
      let s = trimmed;
      const cut = s.search(/[?#]/);
      if (cut >= 0) s = s.slice(0, cut);
      s = s.replace(/\/+$/, '');
      const prefixes = [
        'git+file:', 'git+https://', 'git+http://',
        'https://', 'http://', 'path:', 'github:', 'gitlab:', 'sourcehut:',
      ];
      for (const p of prefixes) {
        if (s.startsWith(p)) { s = s.slice(p.length); break; }
      }
      const parts = s.replace(/^\/+/, '').split('/').filter(Boolean);
      const forge = FORGES.some((p) => trimmed.startsWith(p));
      let name;
      if (forge && parts.length >= 2) name = parts[1];
      else if (parts.length) name = parts[parts.length - 1];
      else name = trimmed;
      return name.replace(/\.git$/, '');
    },

    // Lowercase [a-z0-9-], never empty, no leading digit (utils::plugins::sanitize_id).
    plSanitize(s) {
      const joined = String(s || '').toLowerCase().split(/[^a-z0-9]+/).filter(Boolean).join('-');
      if (!joined) return 'plugin';
      return /^[0-9]/.test(joined) ? 'plugin-' + joined : joined;
    },

    // Prefixes are empty or [a-z][a-z0-9]* (the `-` is added by Neo).
    plSanitizePrefix(s) {
      return String(s || '').toLowerCase().replace(/[^a-z0-9]+/g, '').replace(/^[0-9]+/, '');
    },

    // Repository identity ignoring the branch (utils::plugins::repo_key).
    plRepoKey(url) {
      let base = splitQuery(url).base.replace(/\/+$/, '');
      for (const f of FORGES) {
        if (base.startsWith(f)) {
          return (f + base.slice(f.length).split('/').slice(0, 2).join('/')).toLowerCase();
        }
      }
      for (const p of ['git+file://', 'git+file:', 'file://', 'file:']) {
        if (base.startsWith(p)) return 'git+file:/' + base.slice(p.length).replace(/^\/+/, '');
      }
      return base.replace(/\.git$/, '');
    },

    // Branch / ref in a URL, or null when the scheme has none (path:).
    plRefOf(url) {
      const { base, params } = splitQuery(url);
      const ref = params.find((p) => p.startsWith('ref='));
      if (ref) return decodeURIComponent(ref.slice(4));
      for (const f of FORGES) {
        if (base.startsWith(f)) {
          const parts = base.slice(f.length).replace(/\/+$/, '').split('/');
          return parts.length > 2 ? parts.slice(2).join('/') : '';
        }
      }
      if (/^(git\+|https?:\/\/)/.test(base)) return '';
      return null;
    },

    // URL with its branch set (empty removes it). Forge refs without a slash go in
    // the path (github:o/r/dev), everything else in ?ref=.
    plWithRef(url, ref) {
      const r = String(ref || '').trim();
      const { base, params, frag } = splitQuery(url);
      const rest = params.filter((p) => !p.startsWith('ref='));
      for (const f of FORGES) {
        if (base.startsWith(f)) {
          const repo = base.slice(f.length).replace(/\/+$/, '').split('/').slice(0, 2).join('/');
          if (r && !r.includes('/')) return joinQuery(f + repo + '/' + r, rest, frag);
          return joinQuery(f + repo, r ? rest.concat('ref=' + encodeURIComponent(r)) : rest, frag);
        }
      }
      if (this.plRefOf(url) === null) return String(url || '').trim();
      return joinQuery(base, r ? rest.concat('ref=' + encodeURIComponent(r)) : rest, frag);
    },

    plEntries(optionName) {
      const v = this.values[optionName];
      return v && typeof v === 'object' && !Array.isArray(v) ? v : {};
    },

    plIds(optionName) {
      return Object.keys(this.plEntries(optionName));
    },

    plEntry(optionName, id) {
      return this.plEntries(optionName)[id] || { url: '', prefix: '' };
    },

    plIsSaved(optionName, id) {
      const o = this.originals[optionName];
      return !!(o && typeof o === 'object' && !Array.isArray(o) && o[id]);
    },

    plServices(id) {
      const hit = this.pluginInventory().find((p) => p.id === id);
      return (hit && hit.services) ? hit.services : [];
    },

    plEmptyHint(optionName) {
      return this.optUi(optionName)?.emptyHint
        || 'No plugins yet. Add a flake URL (github:user/repo, git+file:/path, or path:/path).';
    },

    // { <id>: { url, prefix } } from the stored value (old configs: a list of URLs).
    plNormalize(v) {
      if (Array.isArray(v)) {
        const ids = v.map((u) => this.plSanitize(this.pluginLabel(u)));
        const out = {};
        v.forEach((url, i) => {
          const id = ids.filter((x) => x === ids[i]).length > 1 ? `${ids[i]}-${i}` : ids[i];
          out[id] = { url, prefix: '' };
        });
        return out;
      }
      const out = {};
      if (v && typeof v === 'object') {
        Object.keys(v).forEach((id) => {
          out[id] = { url: String(v[id]?.url || ''), prefix: String(v[id]?.prefix || '') };
        });
      }
      return out;
    },

    initPluginList(optionName) {
      this.values[optionName] = this.plNormalize(this.values[optionName]);
      if (this.originals && optionName in this.originals) {
        this.originals[optionName] = this.plNormalize(this.originals[optionName]);
      }
      if (!this.plDraft) this.plDraft = {};
      if (!this.plDraftPrefix) this.plDraftPrefix = {};
      if (!this.plError) this.plError = {};
      if (!this.plEdit) this.plEdit = {};
      this.plDraft[optionName] = this.plDraft[optionName] || '';
      this.plDraftPrefix[optionName] = this.plDraftPrefix[optionName] || '';
      this.plError[optionName] = '';
      this.plEdit[optionName] = null;
    },

    // Instances of the same repository as `url` (other than `exceptId`).
    plSameRepo(optionName, url, exceptId) {
      const key = this.plRepoKey(url);
      return this.plIds(optionName).filter(
        (id) => id !== exceptId && this.plRepoKey(this.plEntry(optionName, id).url) === key
      );
    },

    // Prefix to suggest while typing a URL: the branch name when the plugin is
    // already there. It follows the URL until the user types a prefix themselves.
    plSuggestPrefix(optionName) {
      if (!this.plPrefixAuto) this.plPrefixAuto = {};
      const current = this.plDraftPrefix[optionName] || '';
      if (current && !this.plPrefixAuto[optionName]) return;
      const url = (this.plDraft[optionName] || '').trim();
      const same = url ? this.plSameRepo(optionName, url) : [];
      if (same.length === 0) {
        if (this.plPrefixAuto[optionName]) this.plDraftPrefix[optionName] = '';
        this.plPrefixAuto[optionName] = false;
        return;
      }
      const taken = new Set(same.map((id) => this.plEntry(optionName, id).prefix));
      let p = this.plSanitizePrefix(this.plRefOf(url) || '');
      if (!p || taken.has(p)) {
        const base = p || 'copy';
        let n = 2;
        while (taken.has(base + n)) n += 1;
        p = base + n;
      }
      this.plDraftPrefix[optionName] = p;
      this.plPrefixAuto[optionName] = true;
    },

    plPrefixTyped(optionName) {
      if (!this.plPrefixAuto) this.plPrefixAuto = {};
      this.plPrefixAuto[optionName] = false;
      this.plError[optionName] = '';
    },

    plNeedsPrefix(optionName) {
      const url = (this.plDraft[optionName] || '').trim();
      return !!url && this.plSameRepo(optionName, url).length > 0;
    },

    // Problem with adding `url` under `prefix`, or ''.
    plAddProblem(optionName, url, prefix) {
      if (!url) return 'Enter the plugin address first.';
      if (prefix !== this.plSanitizePrefix(prefix)) {
        return 'The prefix may only have lowercase letters and digits, starting with a letter.';
      }
      const clash = this.plSameRepo(optionName, url)
        .find((id) => this.plEntry(optionName, id).prefix === prefix);
      if (clash) {
        return prefix
          ? `This plugin is already added with the prefix "${prefix}". Pick another prefix.`
          : 'This plugin is already added. Give the new copy a prefix, for example its branch name.';
      }
      return '';
    },

    plNewId(optionName, url, prefix) {
      const base = this.plSanitize(this.pluginLabel(url) + (prefix ? '-' + prefix : ''));
      const ids = new Set(this.plIds(optionName));
      if (!ids.has(base)) return base;
      let n = 2;
      while (ids.has(`${base}-${n}`)) n += 1;
      return `${base}-${n}`;
    },

    addPluginUrl(optionName) {
      const url = (this.plDraft[optionName] || '').trim();
      const prefix = (this.plDraftPrefix[optionName] || '').trim();
      const problem = this.plAddProblem(optionName, url, prefix);
      this.plError[optionName] = url ? problem : '';
      if (problem) return;
      const id = this.plNewId(optionName, url, prefix);
      this.values[optionName] = { ...this.plEntries(optionName), [id]: { url, prefix } };
      this.plDraft[optionName] = '';
      this.plDraftPrefix[optionName] = '';
      this.plError[optionName] = '';
      if (this.plPrefixAuto) this.plPrefixAuto[optionName] = false;
    },

    // ── edit in place ────────────────────────────────────────────────────────
    startPluginEdit(optionName, id) {
      const e = this.plEntry(optionName, id);
      this.plEdit[optionName] = { id, url: e.url, ref: this.plRefOf(e.url) };
    },

    plEditBranchChanged(optionName) {
      const ed = this.plEdit[optionName];
      if (!ed || ed.ref === null) return;
      ed.url = this.plWithRef(ed.url, ed.ref);
    },

    plEditUrlChanged(optionName) {
      const ed = this.plEdit[optionName];
      if (ed) ed.ref = this.plRefOf(ed.url);
    },

    cancelPluginEdit(optionName) {
      this.plEdit[optionName] = null;
    },

    applyPluginEdit(optionName) {
      const ed = this.plEdit[optionName];
      if (!ed) return;
      const url = String(ed.url || '').trim();
      if (!url) {
        this.plError[optionName] = 'The address cannot be empty. Use Remove to take the plugin out.';
        return;
      }
      const prefix = this.plEntry(optionName, ed.id).prefix;
      const clash = this.plSameRepo(optionName, url, ed.id)
        .find((id) => this.plEntry(optionName, id).prefix === prefix);
      if (clash) {
        this.plError[optionName] = `"${clash}" already uses this plugin with the same prefix.`;
        return;
      }
      const next = { ...this.plEntries(optionName) };
      next[ed.id] = { ...next[ed.id], url };
      this.values[optionName] = next;
      this.plEdit[optionName] = null;
      this.plError[optionName] = '';
    },

    // ── remove ───────────────────────────────────────────────────────────────
    async removePlugin(optionName, id) {
      if (!(id in this.plEntries(optionName))) return;
      if (this.plIsSaved(optionName, id)) {
        const ok = await this.confirmPluginRemoval(
          optionName,
          this.pluginRemovalPreviewFor(optionName, [id])
        );
        if (!ok) return;
      }
      const next = { ...this.plEntries(optionName) };
      delete next[id];
      this.values[optionName] = next;
      if (this.plEdit[optionName]?.id === id) this.plEdit[optionName] = null;
    },

    // Settings tables that go with the removed instances: services only they declare.
    pluginRemovalPreviewFor(optionName, idsBeingRemoved) {
      const owners = {};
      this.pluginInventory().forEach((p) => {
        (p.services || []).forEach((svc) => {
          (owners[svc] = owners[svc] || []).push(p.id);
        });
      });
      const drop = new Set(idsBeingRemoved);
      const remaining = new Set(this.plIds(optionName).filter((id) => !drop.has(id)));
      return idsBeingRemoved.map((id) => ({
        id,
        url: this.plEntry(optionName, id).url,
        services: this.plServices(id).filter((svc) => {
          const o = owners[svc] || [];
          return o.length > 0 && o.every((x) => !remaining.has(x));
        }),
      }));
    },

    confirmPluginRemoval(optionName, preview) {
      const dlg = document.getElementById(`plugin-remove-dialog-${optionName}`);
      if (!dlg) {
        const lines = preview.map((p) => {
          const svcs = p.services.length ? p.services.join(', ') : 'no service tables';
          return `${p.id}: ${svcs}`;
        });
        return Promise.resolve(window.confirm(
          'Remove plugin from the list?\n\n' + lines.join('\n') +
          '\n\nAppdata directories are kept. Save afterwards to drop settings.toml tables.'
        ));
      }
      this.plRemovePreview = { ...this.plRemovePreview, [optionName]: preview };
      return new Promise((resolve) => {
        const ok = document.getElementById(`plugin-remove-confirm-${optionName}`);
        const cancel = document.getElementById(`plugin-remove-cancel-${optionName}`);
        let settled = false;
        const done = (val) => {
          if (settled) return;
          settled = true;
          ok?.removeEventListener('click', onOk);
          cancel?.removeEventListener('click', onCancel);
          dlg.removeEventListener('close', onClose);
          resolve(val);
        };
        // Resolve before close(): HTMLDialogElement fires 'close' synchronously,
        // and the harness mirrors that — done() must settle first.
        const onOk = () => { done(true); dlg.close(); };
        const onCancel = () => { done(false); dlg.close(); };
        const onClose = () => done(false);
        ok?.addEventListener('click', onOk);
        cancel?.addEventListener('click', onCancel);
        dlg.addEventListener('close', onClose);
        dlg.showModal();
      });
    },
  };

  const widget = {
    name: 'pluginList',
    mixins,
    init(optionName) { this.initPluginList(optionName); },
    prepareSave(optionName) {
      if (this.isAtOriginal(optionName)) return undefined;
      const out = {};
      Object.entries(this.plEntries(optionName)).forEach(([id, e]) => {
        out[id] = e.prefix ? { url: e.url, prefix: e.prefix } : { url: e.url };
      });
      return out;
    },
  };

  root.NeoWidgets.register(widget.name, widget);
  if (typeof module === 'object' && module.exports) module.exports = widget;
})(typeof globalThis !== 'undefined' ? globalThis : this);
