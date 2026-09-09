// pluginList widget — listOf flake URLs with add/remove and uninstall confirm.
(function (root) {
  if (!root.NeoWidgets) {
    throw new Error('registry.js must load before plugin_list.js');
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

    pluginLabel(url) {
      let s = String(url || '').trim();
      const cut = s.search(/[?#]/);
      if (cut >= 0) s = s.slice(0, cut);
      s = s.replace(/\/+$/, '');
      const prefixes = [
        'git+file:', 'git+https://', 'git+http://',
        'https://', 'http://', 'path:', 'github:', 'gitlab:', 'sourcehut:',
      ];
      for (const p of prefixes) {
        if (s.startsWith(p)) {
          s = s.slice(p.length);
          break;
        }
      }
      s = s.replace(/^\/+/, '');
      const parts = s.split('/').filter(Boolean);
      return parts.length ? parts[parts.length - 1] : String(url || '').trim();
    },

    plNormUrl(url) {
      let s = String(url || '').trim();
      const hash = s.indexOf('#');
      if (hash >= 0) s = s.slice(0, hash);
      let query = '';
      const q = s.indexOf('?');
      if (q >= 0) {
        query = s.slice(q);
        s = s.slice(0, q);
      }
      const prefixes = ['git+file://', 'git+file:', 'file://', 'file:'];
      for (const p of prefixes) {
        if (s.startsWith(p)) {
          return 'git+file:/' + s.slice(p.length).replace(/^\/+/, '') + query;
        }
      }
      return s + query;
    },

    plServices(url) {
      const want = this.plNormUrl(url);
      const hit = this.pluginInventory().find((p) => this.plNormUrl(p.url) === want);
      return (hit && hit.services) ? hit.services : [];
    },

    plEmptyHint(optionName) {
      return this.optUi(optionName)?.emptyHint
        || 'No plugins yet. Add a flake URL (github:user/repo, git+file:/path, or path:/path).';
    },

    initPluginList(optionName) {
      if (!Array.isArray(this.values[optionName])) this.values[optionName] = [];
      if (!this.plDraft) this.plDraft = {};
      this.plDraft[optionName] = this.plDraft[optionName] || '';
    },

    addPluginUrl(optionName) {
      const raw = (this.plDraft[optionName] || '').trim();
      if (!raw) return;
      const list = Array.isArray(this.values[optionName]) ? [...this.values[optionName]] : [];
      if (list.some((u) => this.plNormUrl(u) === this.plNormUrl(raw))) {
        this.plDraft[optionName] = '';
        return;
      }
      list.push(raw);
      this.values[optionName] = list;
      this.plDraft[optionName] = '';
    },

    async removePluginUrl(optionName, idx) {
      const list = Array.isArray(this.values[optionName]) ? this.values[optionName] : [];
      const url = list[idx];
      if (url == null || url === undefined) return;
      const ok = await this.confirmPluginRemoval(
        optionName,
        this.pluginRemovalPreviewFor(optionName, [url])
      );
      if (!ok) return;
      this.removeListItem(optionName, idx);
    },

    pluginRemovalPreviewFor(optionName, urlsBeingRemoved) {
      const inv = this.pluginInventory();
      const owners = {};
      inv.forEach((p) => {
        (p.services || []).forEach((svc) => {
          if (!owners[svc]) owners[svc] = [];
          owners[svc].push(p.url);
        });
      });
      const current = Array.isArray(this.values[optionName]) ? this.values[optionName] : [];
      const originals = Array.isArray(this.originals[optionName]) ? this.originals[optionName] : [];
      const drop = new Set(urlsBeingRemoved.map((u) => this.plNormUrl(u)));
      const nextNorm = new Set(
        current.filter((u) => !drop.has(this.plNormUrl(u))).map((u) => this.plNormUrl(u))
      );
      const removedNorm = originals
        .map((u) => this.plNormUrl(u))
        .filter((u) => !nextNorm.has(u));
      return urlsBeingRemoved.map((url) => ({
        url,
        label: this.pluginLabel(url),
        services: this.plServices(url).filter((svc) => {
          const o = owners[svc] || [];
          return o.length > 0 && o.every((u) => removedNorm.includes(this.plNormUrl(u)));
        }),
      }));
    },

    confirmPluginRemoval(optionName, preview) {
      const esc = (s) => String(s)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
      const dlg = document.getElementById(`plugin-remove-dialog-${optionName}`);
      const body = document.getElementById(`plugin-remove-body-${optionName}`);
      if (!dlg || !body) {
        const lines = preview.map((p) => {
          const svcs = p.services.length ? p.services.join(', ') : 'no service tables';
          return `${p.label}: ${svcs}`;
        });
        return Promise.resolve(window.confirm(
          'Remove plugin from the list?\n\n' + lines.join('\n') +
          '\n\nAppdata directories are kept. Save afterwards to drop settings.toml tables.'
        ));
      }
      body.innerHTML = preview.map((p) => {
        const svcs = p.services.length
          ? `<ul class="list-disc pl-5 mt-1 space-y-0.5">${p.services.map((s) => `<li class="font-mono">${esc(s)}</li>`).join('')}</ul>`
          : '<p class="text-xs opacity-60 mt-1">No settings tables attributed to this plugin.</p>';
        return `<div class="mb-3"><div class="font-semibold font-mono">${esc(p.label)}</div>` +
          `<div class="text-[11px] opacity-60 break-all">${esc(p.url)}</div>${svcs}</div>`;
      }).join('');
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
      return Array.isArray(this.values[optionName]) ? this.values[optionName] : [];
    },
  };

  root.NeoWidgets.register(widget.name, widget);
  if (typeof module === 'object' && module.exports) module.exports = widget;
})(typeof globalThis !== 'undefined' ? globalThis : this);
