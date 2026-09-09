'use strict';

require('./registry.js');
require('./provider_auth.js');
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { makeForm, createDocument } = require('./test/harness.js');

const catalog = [
  { id: 'openai', label: 'OpenAI', hasApiKey: true, hasOauth: false, envVar: 'OPENAI_API_KEY', models: ['gpt-4o'] },
  { id: 'anthropic', label: 'Anthropic', hasApiKey: true, hasOauth: true, oauthFlow: 'pkce', envVar: 'ANTHROPIC_API_KEY', models: ['claude-sonnet'] },
  { id: 'openai-codex', label: 'ChatGPT/Codex', hasApiKey: false, hasOauth: true, oauthFlow: 'external' },
  { id: 'xai', label: 'xAI', hasApiKey: true, hasOauth: true, oauthFlow: 'pkce', envVar: 'XAI_API_KEY', models: ['grok-4'] },
  { id: 'custom', label: 'Custom', hasApiKey: true, hasOauth: false, needsBaseUrl: true, models: [] },
  { id: 'sdk', label: 'SDK', hasApiKey: false, hasOauth: false },
];

function llmOption(current) {
  return {
    name: 'llm',
    type: {
      kind: 'submodule',
      fields: [
        { name: 'provider', type: { kind: 'str' }, description: 'Which vendor', example: 'openai', default: null },
        { name: 'apiKey', type: { kind: 'str' }, description: 'Secret', example: 'sk-...' },
        { name: 'model', type: { kind: 'str' }, description: 'Model id', example: 'grok-4' },
        { name: 'baseUrl', type: { kind: 'str' }, description: 'OpenAI-compatible URL', example: 'http://127.0.0.1:11434/v1' },
      ],
    },
    default: { provider: '', apiKey: '', model: '', baseUrl: '' },
    current: current || { provider: '', apiKey: '', model: '', baseUrl: '' },
    ui: { widget: 'providerAuth', catalog },
  };
}

function okJson(data) {
  return {
    ok: true,
    status: 200,
    async json() { return data; },
  };
}

function errJson(status, data) {
  return {
    ok: false,
    status,
    async json() { return data; },
  };
}

test('registers providerAuth on NeoWidgets', () => {
  const w = globalThis.NeoWidgets.get('providerAuth');
  assert.ok(w);
  assert.equal(w.name, 'providerAuth');
  assert.equal(typeof w.init, 'function');
  assert.equal(typeof w.prepareSave, 'function');
  assert.equal(typeof w.onRevert, 'function');
  assert.equal(typeof w.mixins.paCatalog, 'function');
  assert.equal(typeof w.mixins.paPrepareSave, 'function');
});

test('paCatalog / paRow / paOptionLabel', () => {
  const form = makeForm({ options: [llmOption()] });
  assert.equal(form.paCatalog('llm').length, 6);
  assert.equal(form.paRow('llm'), null);

  form.values.llm.provider = 'openai';
  assert.equal(form.paRow('llm').id, 'openai');
  assert.equal(form.paOptionLabel(form.paRow('llm')), 'OpenAI (API key)');

  form.values.llm.provider = 'anthropic';
  assert.equal(form.paOptionLabel(form.paRow('llm')), 'Anthropic (API key / OAuth)');

  form.values.llm.provider = 'openai-codex';
  assert.equal(form.paOptionLabel(form.paRow('llm')), 'ChatGPT/Codex (OAuth)');

  form.values.llm.provider = 'sdk';
  assert.equal(form.paOptionLabel(form.paRow('llm')), 'SDK');

  assert.equal(form.paOptionLabel(null), '');
});

test('paHint branches', () => {
  const form = makeForm({ options: [llmOption()] });
  assert.match(form.paHint('llm'), /Pick a provider/);

  form.values.llm.provider = 'custom';
  assert.match(form.paHint('llm'), /base URL and a model/);

  form.values.llm.provider = 'anthropic';
  assert.match(form.paHint('llm'), /Paste an API key, or log in with OAuth/);

  form.values.llm.provider = 'openai-codex';
  assert.match(form.paHint('llm'), /uses OAuth \(no API key\)/);

  form.values.llm.provider = 'openai';
  assert.match(form.paHint('llm'), /OPENAI_API_KEY/);

  form.values.llm.provider = 'sdk';
  assert.match(form.paHint('llm'), /No API key or OAuth/);
});

test('paApiKeyPlaceholder', () => {
  const form = makeForm({ options: [llmOption()] });

  form.values.llm.provider = 'custom';
  assert.equal(form.paApiKeyPlaceholder('llm'), 'sk-...');

  const noEx = makeForm({
    options: [{
      ...llmOption(),
      type: {
        kind: 'submodule',
        fields: [
          { name: 'provider', type: { kind: 'str' } },
          { name: 'apiKey', type: { kind: 'str' } },
          { name: 'model', type: { kind: 'str' } },
          { name: 'baseUrl', type: { kind: 'str' } },
        ],
      },
    }],
  });
  noEx.values.llm.provider = 'custom';
  assert.equal(noEx.paApiKeyPlaceholder('llm'), 'optional — many local endpoints need none');

  form.values.llm.provider = 'anthropic';
  assert.equal(form.paApiKeyPlaceholder('llm'), 'leave empty to use OAuth instead');

  form.values.llm.provider = 'openai';
  assert.equal(form.paApiKeyPlaceholder('llm'), 'OPENAI_API_KEY');

  form.values.llm.provider = 'sdk';
  assert.equal(form.paApiKeyPlaceholder('llm'), 'API key');
});

test('paShowBaseUrl only for needsBaseUrl', () => {
  const form = makeForm({ options: [llmOption()] });
  assert.equal(form.paShowBaseUrl('llm'), false);
  form.values.llm.provider = 'openai';
  assert.equal(form.paShowBaseUrl('llm'), false);
  form.values.llm.provider = 'custom';
  assert.equal(form.paShowBaseUrl('llm'), true);
});

test('paEnsure fills missing submodule fields', () => {
  const form = makeForm({ options: [llmOption()] });
  form.values.llm = null;
  const v = form.paEnsure('llm');
  assert.equal(typeof v, 'object');
  assert.equal(v.provider, '');
  assert.equal(v.apiKey, '');
  assert.equal(v.model, '');
  assert.equal(v.baseUrl, '');

  form.values.llm = { provider: 'xai' };
  form.paEnsure('llm');
  assert.equal(form.values.llm.apiKey, '');
  assert.equal(form.values.llm.model, '');
  assert.equal(form.values.llm.baseUrl, '');
  assert.equal(form.values.llm.provider, 'xai');
});

test('paOnProviderChange fills suggested model and oauth status', async () => {
  const calls = [];
  const form = makeForm({
    options: [llmOption()],
    fetch: async (url, init) => {
      calls.push({ url, method: init.method, body: JSON.parse(init.body) });
      return okJson({ ok: true, status: { logged_in: false } });
    },
  });

  form.values.llm.provider = 'openai';
  form.paOnProviderChange('llm');
  assert.equal(form.values.llm.model, 'gpt-4o');
  assert.equal(calls.length, 0);

  form.values.llm.provider = '';
  form.paOnProviderChange('llm');
  assert.equal(form.values.llm.provider, '');

  form.values.llm.model = '';
  form.values.llm.provider = 'anthropic';
  form.paOnProviderChange('llm');
  assert.equal(form.values.llm.model, 'claude-sonnet');
  await new Promise((r) => setImmediate(r));
  assert.equal(calls.length, 1);
  assert.equal(calls[0].body.action, 'status');
  assert.equal(calls[0].body.provider, 'anthropic');

  form.uiState.llm.oauth = { logged_in: true, label: 'keep-me' };
  form.values.llm.provider = 'openai';
  form.paOnProviderChange('llm');
  assert.deepEqual(form.uiState.llm.oauth, {});
});

test('paOnProviderChange keeps non-default model', () => {
  const form = makeForm({ options: [llmOption({ provider: '', apiKey: '', model: 'custom-model', baseUrl: '' })] });
  form.values.llm.provider = 'openai';
  form.paOnProviderChange('llm');
  assert.equal(form.values.llm.model, 'custom-model');
});

test('paUseSuggestedModel', () => {
  const form = makeForm({ options: [llmOption({ provider: 'openai', apiKey: '', model: 'other', baseUrl: '' })] });
  form.paUseSuggestedModel('llm');
  assert.equal(form.values.llm.model, 'gpt-4o');

  form.values.llm.provider = 'custom';
  form.paUseSuggestedModel('llm');
  assert.equal(form.values.llm.model, 'gpt-4o');
});

test('paPrepareSave / collectSave omits defaults and nullishes empty strings', () => {
  const form = makeForm({ options: [llmOption()] });
  assert.equal(form.paPrepareSave('llm'), undefined);
  assert.deepEqual(form.collectSave(), {});

  form.values.llm.model = 'grok-4';
  assert.deepEqual(form.paPrepareSave('llm'), { model: 'grok-4' });
  assert.deepEqual(form.collectSave(), { llm: { model: 'grok-4' } });

  form.values.llm.model = '';
  form.values.llm.provider = 'xai';
  form.values.llm.apiKey = 'secret';
  assert.deepEqual(form.paPrepareSave('llm'), { provider: 'xai', apiKey: 'secret' });

  form.values.llm = { provider: '', apiKey: '', model: '', baseUrl: '' };
  assert.equal(form.paNullish(''), null);
  assert.equal(form.paNullish(undefined), null);
  assert.equal(form.paNullish('x'), 'x');
  assert.equal(form.paPrepareSave('llm'), undefined);

  form.values.llm.provider = 'openai';
  form.values.llm.apiKey = '';
  const prepared = form.paPrepareSave('llm');
  assert.deepEqual(prepared, { provider: 'openai' });
  assert.equal(Object.prototype.hasOwnProperty.call(prepared, 'apiKey'), false);
});

test('paChild / paChildExample', () => {
  const form = makeForm({ options: [llmOption()] });
  assert.equal(form.paChild('llm', 'provider').description, 'Which vendor');
  assert.equal(form.paChildExample('llm', 'apiKey'), 'sk-...');
  assert.equal(form.paChildExample('llm', 'missing'), '');
  assert.equal(form.paChild('llm', 'nope'), null);

  const withObj = makeForm({
    options: [{
      ...llmOption(),
      type: {
        kind: 'submodule',
        fields: [
          { name: 'provider', type: { kind: 'str' }, example: { nested: true } },
          { name: 'apiKey', type: { kind: 'str' } },
          { name: 'model', type: { kind: 'str' } },
          { name: 'baseUrl', type: { kind: 'str' } },
        ],
      },
    }],
  });
  assert.equal(withObj.paChildExample('llm', 'provider'), JSON.stringify({ nested: true }));
});

test('OAuth status on init for hasOauth provider', async () => {
  const calls = [];
  const form = makeForm({
    options: [llmOption({ provider: 'anthropic', apiKey: '', model: '', baseUrl: '' })],
    serviceName: 'hermes',
    isCore: false,
    fetch: async (url, init) => {
      calls.push({ url, method: init.method, body: JSON.parse(init.body) });
      return okJson({ ok: true, status: { logged_in: true, label: 'me' } });
    },
  });

  form.initWidgets();
  await new Promise((r) => setImmediate(r));

  assert.equal(calls.length, 1);
  assert.equal(calls[0].url, '/widget/oauth');
  assert.equal(calls[0].method, 'POST');
  assert.deepEqual(calls[0].body, {
    service: 'hermes',
    option: 'llm',
    is_core: false,
    action: 'status',
    provider: 'anthropic',
  });
  assert.equal(form.paOauthBadge('llm'), 'connected (me)');
  assert.equal(form.paOauthBadgeClass('llm'), 'badge-success');
});

test('OAuth status fetch throw sets error', async () => {
  const form = makeForm({
    options: [llmOption({ provider: 'anthropic', apiKey: '', model: '', baseUrl: '' })],
    fetch: async () => { throw new Error('network down'); },
  });
  await form.paLoadOauthStatus('llm');
  const st = form.paOauthState('llm');
  assert.equal(st.logged_in, false);
  assert.match(st.error, /network down/);
  assert.equal(form.paOauthBadge('llm'), 'not connected');
});

test('OAuth HTTP not ok / body.ok false uses error message', async () => {
  const form = makeForm({
    options: [llmOption({ provider: 'xai', apiKey: '', model: '', baseUrl: '' })],
    fetch: async () => errJson(500, { error: 'boom' }),
  });
  await assert.rejects(() => form.paOauthPost('llm', { action: 'status', provider: 'xai' }), /boom/);

  const form2 = makeForm({
    options: [llmOption({ provider: 'xai', apiKey: '', model: '', baseUrl: '' })],
    fetch: async () => errJson(503, { message: 'unavailable' }),
  });
  await assert.rejects(() => form2.paOauthPost('llm', { action: 'status', provider: 'xai' }), /unavailable/);

  const form3 = makeForm({
    options: [llmOption({ provider: 'xai', apiKey: '', model: '', baseUrl: '' })],
    fetch: async () => errJson(502, {}),
  });
  await assert.rejects(() => form3.paOauthPost('llm', { action: 'status', provider: 'xai' }), /HTTP 502/);

  const form4 = makeForm({
    options: [llmOption({ provider: 'xai', apiKey: '', model: '', baseUrl: '' })],
    fetch: async () => okJson({ ok: false, error: 'denied' }),
  });
  await assert.rejects(() => form4.paOauthPost('llm', { action: 'status', provider: 'xai' }), /denied/);
});

test('paStartOauth external: no dialog, status + toast', async () => {
  const toasts = [];
  const calls = [];
  const doc = createDocument({ pane: { service: 'hermes' } });
  const dlg = doc.set('pa-oauth-dialog-llm');
  const form = makeForm({
    document: doc,
    options: [llmOption({ provider: 'openai-codex', apiKey: '', model: '', baseUrl: '' })],
    neoToast: (msg, kind) => toasts.push({ msg, kind }),
    fetch: async (url, init) => {
      calls.push(JSON.parse(init.body));
      return okJson({ ok: true, status: { logged_in: false } });
    },
  });

  await form.paStartOauth('llm');
  assert.equal(dlg.open, false);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].action, 'status');
  assert.equal(calls[0].provider, 'openai-codex');
  assert.equal(toasts.length, 1);
  assert.match(toasts[0].msg, /external CLI/);
  assert.equal(toasts[0].kind, 'info');
});

test('paStartOauth pkce + paSubmitOauth', async () => {
  const calls = [];
  const doc = createDocument({ pane: { service: 'hermes' } });
  const dlgEl = doc.set('pa-oauth-dialog-llm');
  let phase = 'login';
  const form = makeForm({
    document: doc,
    options: [llmOption({ provider: 'anthropic', apiKey: '', model: 'claude-sonnet', baseUrl: '' })],
    neoToast: () => {},
    fetch: async (url, init) => {
      const body = JSON.parse(init.body);
      calls.push(body);
      if (body.action === 'login') {
        return okJson({
          ok: true,
          flow: 'pkce',
          session_id: 's1',
          auth_url: 'https://example/auth',
        });
      }
      if (body.action === 'submit') {
        return okJson({ ok: true, status: 'approved' });
      }
      if (body.action === 'status') {
        return okJson({ ok: true, status: { logged_in: true, label: 'alice' } });
      }
      return okJson({ ok: true });
    },
  });

  form.uiState.llm = { oauth: {}, dlg: {} };
  await form.paStartOauth('llm');
  assert.equal(dlgEl.open, true);
  const dlg = form.paOauthDlg('llm');
  assert.equal(dlg.url, 'https://example/auth');
  assert.equal(dlg.session, 's1');
  assert.equal(dlg.flow, 'pkce');
  assert.match(dlg.message, /paste the code/i);

  dlg.code = 'auth-code';
  await form.paSubmitOauth('llm');
  assert.ok(calls.some((c) => c.action === 'submit' && c.session === 's1' && c.code === 'auth-code'));
  assert.ok(calls.some((c) => c.action === 'status'));
  assert.equal(form.paOauthState('llm').logged_in, true);
  assert.equal(form.paOauthBadge('llm'), 'connected (alice)');
});

test('paRefreshOauth', async () => {
  const toasts = [];
  const form = makeForm({
    options: [llmOption({ provider: 'xai', apiKey: '', model: '', baseUrl: '' })],
    neoToast: (msg, kind) => toasts.push({ msg, kind }),
    fetch: async (url, init) => {
      const body = JSON.parse(init.body);
      if (body.action === 'refresh') {
        return okJson({ ok: true, status: { logged_in: true, label: 'refreshed' } });
      }
      return okJson({ ok: true });
    },
  });
  form.uiState.llm = { oauth: {} };
  await form.paRefreshOauth('llm');
  assert.equal(form.paOauthState('llm').logged_in, true);
  assert.equal(toasts[0].msg, 'OAuth refreshed');
  assert.equal(toasts[0].kind, 'success');

  const formErr = makeForm({
    options: [llmOption({ provider: 'xai', apiKey: '', model: '', baseUrl: '' })],
    neoToast: (msg, kind) => toasts.push({ msg, kind }),
    fetch: async () => errJson(400, { error: 'refresh failed' }),
  });
  formErr.uiState.llm = { oauth: { logged_in: true } };
  await formErr.paRefreshOauth('llm');
  assert.match(formErr.paOauthState('llm').error, /refresh failed/);
  assert.ok(toasts.some((t) => t.kind === 'error' && /refresh failed/.test(t.msg)));
});

test('paOauthBusy true while in-flight', async () => {
  let resolveFetch;
  const deferred = new Promise((resolve) => { resolveFetch = resolve; });
  const form = makeForm({
    options: [llmOption({ provider: 'anthropic', apiKey: '', model: '', baseUrl: '' })],
    fetch: async () => {
      await deferred;
      return okJson({ ok: true, status: { logged_in: false } });
    },
  });
  const p = form.paLoadOauthStatus('llm');
  assert.equal(form.paOauthBusy('llm'), true);
  resolveFetch();
  await p;
  assert.equal(form.paOauthBusy('llm'), false);
});

test('paOauthDetail copy', () => {
  const form = makeForm({ options: [llmOption({ provider: 'anthropic', apiKey: '', model: '', baseUrl: '' })] });
  form.uiState.llm = {
    oauth: { logged_in: true, source: 'token', expires_at: '2026-12-01' },
  };
  assert.equal(form.paOauthDetail('llm'), 'token · expires 2026-12-01');

  form.uiState.llm.oauth = { logged_in: true };
  assert.match(form.paOauthDetail('llm'), /already configured/);

  form.uiState.llm.oauth = { logged_in: false };
  form.values.llm.provider = 'anthropic';
  assert.match(form.paOauthDetail('llm'), /paste the code/);

  form.values.llm.provider = 'openai-codex';
  assert.match(form.paOauthDetail('llm'), /terminal on the homeserver/);

  // device_code via catalog mutation on a temporary row id
  form.optionsByName.llm.ui.catalog = [
    ...catalog,
    { id: 'devflow', label: 'Device', hasApiKey: false, hasOauth: true, oauthFlow: 'device_code' },
  ];
  form.values.llm.provider = 'devflow';
  assert.match(form.paOauthDetail('llm'), /device code/);
});

test('onRevert re-inits providerAuth', async () => {
  const calls = [];
  const form = makeForm({
    options: [llmOption({ provider: 'anthropic', apiKey: '', model: 'claude-sonnet', baseUrl: '' })],
    fetch: async (url, init) => {
      calls.push(JSON.parse(init.body));
      return okJson({ ok: true, status: { logged_in: true, label: 'bob' } });
    },
  });
  form.values.llm.provider = 'openai';
  form.revertField('llm');
  assert.equal(form.values.llm.provider, 'anthropic');
  await new Promise((r) => setImmediate(r));
  assert.ok(calls.some((c) => c.action === 'status' && c.provider === 'anthropic'));
});
