'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('fs');
const path = require('path');
const vm = require('vm');

const { makeForm, installGlobals } = require('./harness.js');

const optionFormPath = path.join(__dirname, '../../../../static/option_form.js');
const configurationPath = path.join(__dirname, '../../../configuration.html.hbs');
const widgetsDir = path.join(__dirname, '..');

function loadWidgets() {
  const files = [
    'registry.js',
    'exclusive_list_pair.js',
    'plugin_list.js',
    'primary_item_list.js',
    'provider_auth.js',
  ];
  for (const f of files) {
    const p = path.join(widgetsDir, f);
    delete require.cache[require.resolve(p)];
    require(p);
  }
}

function loadOptionForm() {
  loadWidgets();
  const src = fs.readFileSync(optionFormPath, 'utf8');
  vm.runInThisContext(src, { filename: 'option_form.js' });
}

test('option_form.js no longer defines widget methods', () => {
  const src = fs.readFileSync(optionFormPath, 'utf8');
  for (const needle of [
    'elpModes(',
    'elpPrepareSave(',
    'paCatalog(',
    'paPrepareSave(',
    'pilSetPrimary(',
    'pluginInventory(',
    'addPluginUrl(',
    'initExclusiveListPair(',
    'initPluginList(',
    'initProviderAuth(',
  ]) {
    assert.equal(src.includes(needle), false, `option_form.js still contains ${needle}`);
  }
  assert.equal(src.includes('function neoWidget'), true);
  assert.equal(src.includes('NeoWidgets.mixins()'), true);
});

test('configuration.html.hbs loads registry then each widget then option_form.js', () => {
  const html = fs.readFileSync(configurationPath, 'utf8');
  const scripts = [
    '/static/widgets/registry.js',
    '/static/widgets/exclusive_list_pair.js',
    '/static/widgets/plugin_list.js',
    '/static/widgets/primary_item_list.js',
    '/static/widgets/provider_auth.js',
    '/static/option_form.js',
  ];
  let last = -1;
  for (const src of scripts) {
    const i = html.indexOf(src);
    assert.ok(i >= 0, `missing script ${src}`);
    assert.ok(i > last, `${src} must load after the previous widget script`);
    last = i;
  }
});

test('optionForm mixes registered widget methods onto the host', () => {
  installGlobals({});
  loadOptionForm();
  const form = optionForm();
  assert.equal(typeof form.elpModes, 'function');
  assert.equal(typeof form.setElpMode, 'function');
  assert.equal(typeof form.pilSetPrimary, 'function');
  assert.equal(typeof form.addPluginUrl, 'function');
  assert.equal(typeof form.pluginLabel, 'function');
  assert.equal(typeof form.paHint, 'function');
  assert.equal(typeof form.paPrepareSave, 'function');
});

test('save() posts exclusiveListPair payload from widget.prepareSave', async () => {
  const posts = [];
  const document = installGlobals({
    fetch: async (url, init) => {
      posts.push({ url, body: JSON.parse(init.body) });
      return { ok: true, json: async () => ({}), text: async () => '' };
    },
  });
  document.set('options-pane', {
    dataset: { service: 'tinyauth', saveEndpoint: '/save/tinyauth' },
  });
  loadOptionForm();
  const form = optionForm();
  form.serviceName = 'tinyauth';
  form.optionsByName = {
    users: { type: { kind: 'listOf', elem: { kind: 'str' } } },
    access: {
      name: 'access',
      type: {
        kind: 'attrsOf',
        elem: {
          kind: 'submodule',
          fields: [
            { name: 'allow', type: { kind: 'listOf', elem: { kind: 'str' }, values: ['immich'] } },
            { name: 'block', type: { kind: 'listOf', elem: { kind: 'str' }, values: ['immich'] } },
          ],
        },
      },
      ui: {
        widget: 'exclusiveListPair',
        keysFrom: { option: 'users', extract: 'beforeColon' },
        modes: [
          { id: 'open', label: 'All apps', active: [] },
          { id: 'allow', label: 'Allow list', active: ['allow'] },
          { id: 'block', label: 'Block list', active: ['block'] },
        ],
        save: { pruneEmptyEntries: true, omitIfEmpty: true },
      },
    },
  };
  form.values.users = ['alice:HASH', 'bob:HASH'];
  form.values.access = {
    alice: { allow: ['immich'], block: [] },
    bob: { allow: [], block: [] },
  };
  form.defaults.access = {};
  form.originals.access = form.cloneValue(form.values.access);
  form.initWidgets();

  await form.save();
  assert.equal(posts.length, 1);
  assert.equal(posts[0].url, '/save/tinyauth');
  assert.deepEqual(posts[0].body.access, { alice: { allow: ['immich'], block: [] } });
  assert.equal('bob' in (posts[0].body.access || {}), false);
});

test('save() includes empty pluginList when the list was cleared', async () => {
  const posts = [];
  const document = installGlobals({
    fetch: async (url, init) => {
      posts.push({ url, body: JSON.parse(init.body) });
      return { ok: true, json: async () => ({}), text: async () => '' };
    },
  });
  document.set('options-pane', {
    dataset: { service: 'core', saveEndpoint: '/save-core/x' },
  });
  loadOptionForm();
  const form = optionForm();
  form.serviceName = 'core';
  form.isCore = true;
  form.optionsByName = {
    plugins: {
      name: 'plugins',
      type: { kind: 'listOf', elem: { kind: 'str' } },
      ui: { widget: 'pluginList' },
    },
  };
  form.values.plugins = [];
  form.defaults.plugins = [];
  form.originals.plugins = ['github:madebydamo/old'];
  await form.save();
  assert.equal(posts.length, 1);
  assert.deepEqual(posts[0].body.plugins, []);
});

test('initForm does not report dirty after providerAuth canonicalizes Nix nulls', () => {
  const seed = [{
    name: 'llm',
    type: {
      kind: 'submodule',
      fields: [
        { name: 'provider', type: { kind: 'nullOr', elem: { kind: 'enum', values: ['xai'] } }, default: null },
        { name: 'apiKey', type: { kind: 'nullOr', elem: { kind: 'str' } }, default: null },
        { name: 'model', type: { kind: 'nullOr', elem: { kind: 'str' } }, default: 'grok-build-latest' },
        { name: 'baseUrl', type: { kind: 'nullOr', elem: { kind: 'str' } }, default: null },
      ],
    },
    default: { provider: null, apiKey: null, model: 'grok-build-latest', baseUrl: null },
    current: { provider: 'xai', apiKey: 'secret', model: 'grok-build-latest', baseUrl: null },
    ui: {
      widget: 'providerAuth',
      catalog: [
        { id: 'xai', label: 'xAI', hasApiKey: true, hasOauth: true, oauthFlow: 'pkce', models: ['grok-4'] },
      ],
    },
  }];
  const document = installGlobals({
    fetch: async () => ({
      ok: true,
      status: 200,
      json: async () => ({ ok: true, status: { logged_in: false } }),
    }),
  });
  document.set('options-seed', { textContent: JSON.stringify(seed) });
  document.set('options-pane', {
    dataset: { service: 'hermes', saveEndpoint: '/save/hermes' },
  });
  loadOptionForm();
  const form = optionForm();
  form.initForm();
  assert.equal(form.values.llm.baseUrl, '');
  assert.equal(form.isAtOriginal('llm'), true);
});

test('makeForm collectSave still matches optionForm widget dispatch', () => {
  loadWidgets();
  const options = [{
    name: 'telegramAllowedUserId',
    type: { kind: 'listOf', elem: { kind: 'int' } },
    default: [],
    current: [1, 2],
    ui: { widget: 'primaryItemList', entryLabel: 'Home channel' },
  }];
  const isolated = makeForm({ options });
  isolated.initWidgets();
  isolated.pilSetPrimary('telegramAllowedUserId', 1);
  assert.deepEqual(isolated.values.telegramAllowedUserId, [2, 1]);
  assert.deepEqual(isolated.collectSave().telegramAllowedUserId, [2, 1]);
});
