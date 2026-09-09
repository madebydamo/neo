// providerAuth — composite provider + API key and/or OAuth + model editor.
// Colocated with provider_auth.html.hbs. Register via NeoWidgets; mixins attach to the option form host.
(function (root) {
  if (!root.NeoWidgets) {
    throw new Error('registry.js must load before provider_auth.js');
  }

  const mixins = {
    paCatalog(optionName) {
      const cat = this.optUi(optionName)?.catalog;
      return Array.isArray(cat) ? cat : [];
    },

    paRow(optionName) {
      const id = this.values[optionName]?.provider;
      if (!id) return null;
      return this.paCatalog(optionName).find((r) => r.id === id) || null;
    },

    paOptionLabel(row) {
      if (!row) return '';
      const bits = [];
      if (row.hasApiKey) bits.push('API key');
      if (row.hasOauth) bits.push('OAuth');
      const extra = bits.length ? ` (${bits.join(' / ')})` : '';
      return `${row.label || row.id}${extra}`;
    },

    paChild(optionName, fieldName) {
      const fields = this.optType(optionName)?.fields || [];
      return fields.find((f) => f.name === fieldName) || null;
    },

    paChildExample(optionName, fieldName) {
      const ex = this.paChild(optionName, fieldName)?.example;
      if (ex == null || ex === '') return '';
      if (typeof ex === 'string' || typeof ex === 'number' || typeof ex === 'boolean') {
        return String(ex);
      }
      try {
        return JSON.stringify(ex);
      } catch {
        return String(ex);
      }
    },

    paApiKeyPlaceholder(optionName) {
      const row = this.paRow(optionName);
      if (row?.needsBaseUrl) {
        return this.paChildExample(optionName, 'apiKey') || 'optional — many local endpoints need none';
      }
      if (row?.hasOauth) return 'leave empty to use OAuth instead';
      return row?.envVar || 'API key';
    },

    paHint(optionName) {
      const row = this.paRow(optionName);
      if (!row) {
        return 'Pick a provider. API-key vendors get a key field; ChatGPT/Codex, SuperGrok, Nous, and similar use OAuth.';
      }
      if (row.needsBaseUrl) {
        return 'Set the OpenAI-compatible base URL and a model. Paste an API key if the endpoint requires one.';
      }
      if (row.hasApiKey && row.hasOauth) {
        return 'Paste an API key, or log in with OAuth. You do not need both.';
      }
      if (row.hasOauth && !row.hasApiKey) {
        return 'This provider uses OAuth (no API key). Log in below, then set a model.';
      }
      if (row.hasApiKey) {
        return `Paste the ${row.envVar || 'API'} key and set a model.`;
      }
      return 'No API key or OAuth for this provider (SDK, keyless, or local). Set a model if needed.';
    },

    paShowBaseUrl(optionName) {
      const row = this.paRow(optionName);
      return !!(row && row.needsBaseUrl);
    },

    paEnsure(optionName) {
      const cur = this.values[optionName];
      if (!cur || typeof cur !== 'object' || Array.isArray(cur)) {
        this.values[optionName] = this.mergeSubmoduleValue({}, this.optType(optionName) || { kind: 'submodule' });
      }
      const v = this.values[optionName];
      if (v.provider == null) v.provider = '';
      if (v.apiKey == null) v.apiKey = '';
      if (v.model == null) v.model = '';
      if (v.baseUrl == null) v.baseUrl = '';
      return v;
    },

    paOauthState(optionName) {
      const st = this.uiState[optionName] || {};
      return st.oauth || {};
    },

    paOauthDlg(optionName) {
      const st = this.uiState[optionName] || {};
      if (!st.dlg) st.dlg = {};
      this.uiState[optionName] = st;
      return st.dlg;
    },

    paOauthBusy(optionName) {
      return !!this.oauthBusy[optionName];
    },

    paOauthBadge(optionName) {
      const st = this.paOauthState(optionName);
      if (st.loading) return 'checking…';
      if (st.logged_in) return st.label ? `connected (${st.label})` : 'connected';
      if (st.error && !st.logged_in) return 'not connected';
      return 'not connected';
    },

    paOauthBadgeClass(optionName) {
      const st = this.paOauthState(optionName);
      if (st.logged_in) return 'badge-success';
      if (st.loading) return 'badge-ghost';
      return 'badge-ghost';
    },

    paOauthDetail(optionName) {
      const st = this.paOauthState(optionName);
      if (st.logged_in) {
        const bits = [];
        if (st.source) bits.push(st.source);
        if (st.expires_at) bits.push('expires ' + st.expires_at);
        return bits.join(' · ') || 'OAuth is already configured for this provider.';
      }
      const row = this.paRow(optionName);
      if (row?.oauthFlow === 'pkce') {
        return 'Log in: open the authorization page, then paste the code here.';
      }
      if (row?.oauthFlow === 'device_code') {
        return 'Log in: open the authorization page and enter the device code shown in the dialog.';
      }
      if (row?.oauthFlow === 'external') {
        return 'Log in from a terminal on the homeserver, then check status here.';
      }
      return '';
    },

    initProviderAuth(optionName) {
      this.paEnsure(optionName);
      if (!this.uiState[optionName]) this.uiState[optionName] = {};
      this.uiState[optionName].oauth = { loading: false };
      this.uiState[optionName].dlg = {};
      if (this.paRow(optionName)?.hasOauth) {
        this.paLoadOauthStatus(optionName);
      }
    },

    paOnProviderChange(optionName) {
      const v = this.paEnsure(optionName);
      if (v.provider === '') v.provider = null;
      this.values[optionName] = { ...v, provider: v.provider || '' };
      const row = this.paRow(optionName);
      if (row?.models?.length && (!v.model || v.model === this.defaults[optionName]?.model)) {
        v.model = row.models[0];
        this.values[optionName] = { ...v };
      }
      if (row?.hasOauth) {
        this.paLoadOauthStatus(optionName);
      } else if (this.uiState[optionName]) {
        this.uiState[optionName].oauth = {};
      }
    },

    paUseSuggestedModel(optionName) {
      const row = this.paRow(optionName);
      if (!row?.models?.length) return;
      const v = this.paEnsure(optionName);
      v.model = row.models[0];
      this.values[optionName] = { ...v };
    },

    paNullish(v) {
      if (v === '' || v === undefined) return null;
      return v;
    },

    paPrepareSave(optionName) {
      const v = this.paEnsure(optionName);
      const d = this.defaults[optionName] || {};
      const out = {};
      ['provider', 'apiKey', 'model', 'baseUrl'].forEach((k) => {
        const cur = this.paNullish(v[k]);
        const def = this.paNullish(d[k]);
        if (!this.deepEqual(cur, def)) out[k] = cur;
      });
      if (Object.keys(out).length === 0) return undefined;
      return out;
    },

    async paOauthPost(optionName, payload) {
      const pane = document.getElementById('options-pane');
      const service = this.serviceName || pane?.dataset?.service || '';
      const res = await fetch('/widget/oauth', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          service,
          option: optionName,
          is_core: this.isCore,
          ...payload,
        }),
      });
      const body = await res.json().catch(() => ({}));
      if (!res.ok || body.ok === false) {
        const err = body.error || body.message || (`HTTP ${res.status}`);
        throw new Error(err);
      }
      return body;
    },

    async paLoadOauthStatus(optionName) {
      const row = this.paRow(optionName);
      if (!row?.hasOauth) return;
      if (!this.uiState[optionName]) this.uiState[optionName] = {};
      this.uiState[optionName].oauth = { ...(this.uiState[optionName].oauth || {}), loading: true };
      this.oauthBusy = { ...this.oauthBusy, [optionName]: true };
      try {
        const body = await this.paOauthPost(optionName, {
          action: 'status',
          provider: row.id,
        });
        this.uiState[optionName].oauth = { loading: false, ...(body.status || body) };
      } catch (e) {
        this.uiState[optionName].oauth = {
          loading: false,
          logged_in: false,
          error: String(e.message || e),
        };
      } finally {
        this.oauthBusy = { ...this.oauthBusy, [optionName]: false };
      }
    },

    async paRefreshOauth(optionName) {
      const row = this.paRow(optionName);
      if (!row?.hasOauth) return;
      this.oauthBusy = { ...this.oauthBusy, [optionName]: true };
      try {
        const body = await this.paOauthPost(optionName, {
          action: 'refresh',
          provider: row.id,
        });
        this.uiState[optionName].oauth = { loading: false, ...(body.status || body) };
        if (typeof window.neoToast === 'function') {
          window.neoToast(body.status?.logged_in ? 'OAuth refreshed' : 'OAuth not connected', 'success');
        }
      } catch (e) {
        this.uiState[optionName].oauth = {
          ...(this.uiState[optionName].oauth || {}),
          error: String(e.message || e),
        };
        if (typeof window.neoToast === 'function') {
          window.neoToast(String(e.message || e), 'error');
        }
      } finally {
        this.oauthBusy = { ...this.oauthBusy, [optionName]: false };
      }
    },

    paStopPoll(optionName) {
      const st = this.uiState[optionName];
      if (st?.pollTimer) {
        clearInterval(st.pollTimer);
        st.pollTimer = null;
      }
    },

    async paStartOauth(optionName) {
      const row = this.paRow(optionName);
      if (!row?.hasOauth) return;
      if (row.oauthFlow === 'external') {
        if (typeof window.neoToast === 'function') {
          window.neoToast('This provider uses an external CLI on the homeserver.', 'info');
        }
        return this.paLoadOauthStatus(optionName);
      }
      this.oauthBusy = { ...this.oauthBusy, [optionName]: true };
      const dlg = document.getElementById(`pa-oauth-dialog-${optionName}`);
      this.uiState[optionName].dlg = {
        title: `Log in to ${row.label || row.id}`,
        flow: row.oauthFlow,
        status: 'pending',
        message: 'Starting login…',
        error: '',
        url: '',
        userCode: '',
        code: '',
        session: '',
      };
      dlg?.showModal();
      try {
        const body = await this.paOauthPost(optionName, {
          action: 'login',
          provider: row.id,
        });
        this.uiState[optionName].dlg = {
          ...this.uiState[optionName].dlg,
          flow: body.flow || row.oauthFlow,
          session: body.session_id,
          url: body.verification_url || body.auth_url || '',
          userCode: body.user_code || '',
          message: body.flow === 'pkce'
            ? 'Open the page, authorize, then paste the code.'
            : 'Open the page and enter the code if asked.',
          status: 'pending',
          error: '',
        };
        if ((body.flow || row.oauthFlow) === 'device_code' && body.session_id) {
          this.paBeginPoll(optionName, body.session_id);
        }
      } catch (e) {
        this.uiState[optionName].dlg.error = String(e.message || e);
        this.uiState[optionName].dlg.message = '';
      } finally {
        this.oauthBusy = { ...this.oauthBusy, [optionName]: false };
      }
    },

    paBeginPoll(optionName, sessionId) {
      this.paStopPoll(optionName);
      const tick = async () => {
        try {
          const body = await this.paOauthPost(optionName, {
            action: 'poll',
            session: sessionId,
          });
          const dlg = this.uiState[optionName].dlg || {};
          dlg.status = body.status || 'pending';
          dlg.error = body.error_message || '';
          this.uiState[optionName].dlg = { ...dlg };
          if (dlg.status === 'approved') {
            this.paStopPoll(optionName);
            await this.paLoadOauthStatus(optionName);
            if (typeof window.neoToast === 'function') {
              window.neoToast('OAuth login complete', 'success');
            }
          } else if (dlg.status === 'error' || dlg.status === 'expired' || dlg.status === 'denied') {
            this.paStopPoll(optionName);
          }
        } catch (e) {
          this.uiState[optionName].dlg = {
            ...(this.uiState[optionName].dlg || {}),
            error: String(e.message || e),
          };
          this.paStopPoll(optionName);
        }
      };
      this.uiState[optionName].pollTimer = setInterval(tick, 2000);
      tick();
    },

    async paSubmitOauth(optionName) {
      const dlg = this.uiState[optionName]?.dlg || {};
      if (!dlg.session || !dlg.code) return;
      this.oauthBusy = { ...this.oauthBusy, [optionName]: true };
      try {
        const body = await this.paOauthPost(optionName, {
          action: 'submit',
          session: dlg.session,
          code: dlg.code,
        });
        dlg.status = body.status || 'approved';
        dlg.error = body.error || body.message || '';
        this.uiState[optionName].dlg = { ...dlg };
        if (dlg.status === 'approved') {
          await this.paLoadOauthStatus(optionName);
          if (typeof window.neoToast === 'function') {
            window.neoToast('OAuth login complete', 'success');
          }
        }
      } catch (e) {
        this.uiState[optionName].dlg = {
          ...dlg,
          error: String(e.message || e),
        };
      } finally {
        this.oauthBusy = { ...this.oauthBusy, [optionName]: false };
      }
    },
  };

  const widget = {
    name: 'providerAuth',
    mixins,
    init(optionName) { this.initProviderAuth(optionName); },
    prepareSave(optionName) { return this.paPrepareSave(optionName); },
    onRevert(optionName) { this.initProviderAuth(optionName); },
  };

  root.NeoWidgets.register(widget.name, widget);
  if (typeof module === 'object' && module.exports) module.exports = widget;
})(typeof globalThis !== 'undefined' ? globalThis : this);
