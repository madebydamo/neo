'use strict';

require('./registry.js');
require('./plugin_list.js');
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { makeForm, createDocument } = require('./test/harness.js');

const inventory = [
  { url: 'github:madebydamo/neo-foo', services: ['foo', 'foo-worker'] },
  { url: 'git+file:///opt/bar', services: ['bar'] },
  { url: 'github:madebydamo/neo-shared', services: ['foo'] },
];

function pluginsOption(current) {
  return {
    name: 'plugins',
    type: { kind: 'listOf', elem: { kind: 'str' } },
    default: [],
    current: current || [],
    ui: { widget: 'pluginList', emptyHint: 'No plugins yet.' },
  };
}

function formWithPlugins(opts) {
  opts = opts || {};
  const current = opts.current !== undefined ? opts.current : [];
  const option = pluginsOption(current);
  if (opts.emptyHint === null) {
    delete option.ui.emptyHint;
  } else if (opts.emptyHint !== undefined) {
    option.ui.emptyHint = opts.emptyHint;
  }
  const docOpts = {
    pluginInventory: opts.pluginInventory !== undefined ? opts.pluginInventory : inventory,
    confirm: opts.confirm,
  };
  if (opts.document) {
    docOpts.document = opts.document;
  }
  const form = makeForm({
    ...docOpts,
    options: [option],
    values: opts.values,
    originals: opts.originals,
    defaults: opts.defaults,
    pluginInv: opts.pluginInv,
  });
  form.initWidgets();
  return form;
}

// ── pluginLabel ──────────────────────────────────────────────────────────────

test('pluginLabel: github:user/repo → repo', () => {
  const form = formWithPlugins();
  assert.equal(form.pluginLabel('github:user/repo'), 'repo');
});

test('pluginLabel: git+https last segment keeps .git', () => {
  const form = formWithPlugins();
  assert.equal(form.pluginLabel('git+https://host/a/b.git'), 'b.git');
});

test('pluginLabel: strips ?rev= and #ref, trailing slashes', () => {
  const form = formWithPlugins();
  assert.equal(form.pluginLabel('github:user/repo?rev=abc'), 'repo');
  assert.equal(form.pluginLabel('github:user/repo#ref'), 'repo');
  assert.equal(form.pluginLabel('github:user/repo/'), 'repo');
  assert.equal(form.pluginLabel('github:user/repo/?rev=x#y'), 'repo');
});

test('pluginLabel: path:/abs/plugin → last segment', () => {
  const form = formWithPlugins();
  assert.equal(form.pluginLabel('path:/abs/plugin'), 'plugin');
});

test('pluginLabel: empty/undefined → empty-ish string', () => {
  const form = formWithPlugins();
  assert.equal(form.pluginLabel(''), '');
  assert.equal(form.pluginLabel(undefined), '');
  assert.equal(form.pluginLabel(null), '');
});

// ── plNormUrl ────────────────────────────────────────────────────────────────

test('plNormUrl: file: / git+file: variants collapse', () => {
  const form = formWithPlugins();
  const a = form.plNormUrl('git+file:///opt/x');
  const b = form.plNormUrl('file:///opt/x');
  const c = form.plNormUrl('git+file:/opt/x');
  assert.equal(a, 'git+file:/opt/x');
  assert.equal(b, a);
  assert.equal(c, a);
});

test('plNormUrl: hash stripped, query kept; file vs git+file equal', () => {
  const form = formWithPlugins();
  assert.equal(form.plNormUrl('git+file:///opt/x#frag'), 'git+file:/opt/x');
  assert.equal(form.plNormUrl('git+file:///opt/x?rev=1'), 'git+file:/opt/x?rev=1');
  assert.equal(
    form.plNormUrl('file:///opt/plugin'),
    form.plNormUrl('git+file:///opt/plugin')
  );
});

// ── init ─────────────────────────────────────────────────────────────────────

test('init: missing values become []; plDraft set to empty string', () => {
  const form = makeForm({
    pluginInventory: inventory,
    options: [pluginsOption(['github:a/b'])],
    values: { plugins: undefined },
  });
  // makeForm may have already filled from option.current; force missing then init
  delete form.values.plugins;
  form.plDraft = {};
  form.initWidgets();
  assert.deepEqual(form.values.plugins, []);
  assert.equal(form.plDraft.plugins, '');
});

test('init: preserves existing plDraft when already set', () => {
  const form = makeForm({
    pluginInventory: inventory,
    options: [pluginsOption([])],
  });
  form.plDraft.plugins = 'github:keep/me';
  form.initWidgets();
  assert.equal(form.plDraft.plugins, 'github:keep/me');
});

// ── addPluginUrl ─────────────────────────────────────────────────────────────

test('addPluginUrl: empty draft is no-op', () => {
  const form = formWithPlugins({ current: ['github:a/one'] });
  form.plDraft.plugins = '   ';
  form.addPluginUrl('plugins');
  assert.deepEqual(form.values.plugins, ['github:a/one']);
  assert.equal(form.plDraft.plugins, '   ');
});

test('addPluginUrl: trims, appends, clears draft', () => {
  const form = formWithPlugins({ current: [] });
  form.plDraft.plugins = '  github:user/new  ';
  form.addPluginUrl('plugins');
  assert.deepEqual(form.values.plugins, ['github:user/new']);
  assert.equal(form.plDraft.plugins, '');
});

test('addPluginUrl: duplicate by plNormUrl ignored and draft cleared', () => {
  const form = formWithPlugins({
    current: ['git+file:///opt/x'],
  });
  form.plDraft.plugins = 'file:///opt/x';
  form.addPluginUrl('plugins');
  assert.deepEqual(form.values.plugins, ['git+file:///opt/x']);
  assert.equal(form.plDraft.plugins, '');
});

// ── prepareSave / collectSave ────────────────────────────────────────────────

test('collectSave: unchanged list omits plugins key', () => {
  const form = formWithPlugins({
    current: ['github:a/one'],
  });
  assert.equal('plugins' in form.collectSave(), false);
});

test('collectSave: after add includes plugins array', () => {
  const form = formWithPlugins({ current: [] });
  form.plDraft.plugins = 'github:user/new';
  form.addPluginUrl('plugins');
  assert.deepEqual(form.collectSave().plugins, ['github:user/new']);
});

test('collectSave: clearing all plugins saves empty array (vs originals, not defaults)', () => {
  const form = formWithPlugins({
    current: ['github:a/one'],
    defaults: { plugins: [] },
  });
  // originals = ['github:a/one']; remove all → must save []
  form.values.plugins = [];
  const saved = form.collectSave();
  assert.ok('plugins' in saved);
  assert.deepEqual(saved.plugins, []);
});

test('prepareSave: returns undefined when at original', () => {
  const form = formWithPlugins({ current: ['github:a/one'] });
  const w = globalThis.NeoWidgets.get('pluginList');
  assert.equal(w.prepareSave.call(form, 'plugins'), undefined);
});

test('prepareSave: non-array values become []', () => {
  const form = formWithPlugins({ current: ['github:a/one'] });
  form.values.plugins = null;
  const w = globalThis.NeoWidgets.get('pluginList');
  assert.deepEqual(w.prepareSave.call(form, 'plugins'), []);
});

// ── plServices ───────────────────────────────────────────────────────────────

test('plServices: lookup by normalized URL; unknown → []', () => {
  const form = formWithPlugins();
  assert.deepEqual(form.plServices('github:madebydamo/neo-foo'), ['foo', 'foo-worker']);
  assert.deepEqual(form.plServices('file:///opt/bar'), ['bar']);
  assert.deepEqual(form.plServices('github:nobody/missing'), []);
});

test('pluginInventory: reads seed when _pluginInv is null', () => {
  const form = formWithPlugins({ pluginInv: null });
  assert.equal(form._pluginInv, null);
  const inv = form.pluginInventory();
  assert.equal(inv.length, 3);
  assert.equal(form._pluginInv.length, 3);
});

// ── pluginRemovalPreviewFor — last owner ─────────────────────────────────────

test('pluginRemovalPreviewFor: shared service not deleted when co-owner remains', () => {
  const form = formWithPlugins({
    current: [
      'github:madebydamo/neo-foo',
      'github:madebydamo/neo-shared',
      'git+file:///opt/bar',
    ],
  });
  const preview = form.pluginRemovalPreviewFor('plugins', ['github:madebydamo/neo-foo']);
  assert.equal(preview.length, 1);
  assert.equal(preview[0].label, 'neo-foo');
  // foo still owned by neo-shared; foo-worker only by neo-foo → listed
  assert.deepEqual(preview[0].services, ['foo-worker']);
  assert.ok(!preview[0].services.includes('foo'));
});

test('pluginRemovalPreviewFor: removing both owners lists shared service', () => {
  const form = formWithPlugins({
    current: [
      'github:madebydamo/neo-foo',
      'github:madebydamo/neo-shared',
    ],
  });
  const preview = form.pluginRemovalPreviewFor('plugins', [
    'github:madebydamo/neo-foo',
    'github:madebydamo/neo-shared',
  ]);
  const byUrl = Object.fromEntries(preview.map((p) => [p.url, p]));
  assert.ok(byUrl['github:madebydamo/neo-foo'].services.includes('foo'));
  assert.ok(byUrl['github:madebydamo/neo-shared'].services.includes('foo'));
});

test('pluginRemovalPreviewFor: sole owner lists its service', () => {
  const form = formWithPlugins({
    current: ['git+file:///opt/bar', 'github:madebydamo/neo-foo'],
  });
  const preview = form.pluginRemovalPreviewFor('plugins', ['git+file:///opt/bar']);
  assert.deepEqual(preview[0].services, ['bar']);
});

// ── pluginRemovalPreviewFor vs originals ─────────────────────────────────────

test('pluginRemovalPreviewFor: session-added plugin still gets a preview row; services not flagged unless owners in originals-removed', () => {
  const form = formWithPlugins({
    current: ['github:madebydamo/neo-shared'],
  });
  // Add neo-foo in-session (not in originals), then preview removing it
  form.values.plugins = [
    'github:madebydamo/neo-shared',
    'github:madebydamo/neo-foo',
  ];
  const preview = form.pluginRemovalPreviewFor('plugins', ['github:madebydamo/neo-foo']);
  assert.equal(preview.length, 1);
  assert.equal(preview[0].url, 'github:madebydamo/neo-foo');
  assert.equal(preview[0].label, 'neo-foo');
  // neo-foo was never in originals, so removedNorm does not include it —
  // foo-worker (sole owner neo-foo) and foo (shared) must not be flagged.
  assert.deepEqual(preview[0].services, []);
});

// ── confirmPluginRemoval with dialog ─────────────────────────────────────────

test('confirmPluginRemoval dialog: confirm removes url; HTML-escapes label/url', async () => {
  const document = createDocument({ pluginInventory: inventory });
  const dlg = document.set('plugin-remove-dialog-plugins');
  const body = document.set('plugin-remove-body-plugins');
  const okBtn = document.set('plugin-remove-confirm-plugins');
  document.set('plugin-remove-cancel-plugins');

  const form = formWithPlugins({
    document,
    current: ['github:user/a&b<"x">'],
  });

  const removePromise = form.removePluginUrl('plugins', 0);
  assert.equal(dlg.open, true);
  assert.match(body.innerHTML, /a&amp;b&lt;&quot;x&quot;&gt;/);
  assert.match(body.innerHTML, /github:user\/a&amp;b&lt;&quot;x&quot;&gt;/);

  okBtn.click();
  await removePromise;
  assert.deepEqual(form.values.plugins, []);
});

test('confirmPluginRemoval dialog: cancel leaves list unchanged', async () => {
  const document = createDocument({ pluginInventory: inventory });
  document.set('plugin-remove-dialog-plugins');
  document.set('plugin-remove-body-plugins');
  document.set('plugin-remove-confirm-plugins');
  const cancelBtn = document.set('plugin-remove-cancel-plugins');

  const form = formWithPlugins({
    document,
    current: ['github:madebydamo/neo-foo'],
  });

  const removePromise = form.removePluginUrl('plugins', 0);
  cancelBtn.click();
  await removePromise;
  assert.deepEqual(form.values.plugins, ['github:madebydamo/neo-foo']);
});

test('confirmPluginRemoval dialog: fills services list in body', async () => {
  const document = createDocument({ pluginInventory: inventory });
  const dlg = document.set('plugin-remove-dialog-plugins');
  const body = document.set('plugin-remove-body-plugins');
  document.set('plugin-remove-confirm-plugins');
  const cancelBtn = document.set('plugin-remove-cancel-plugins');

  const form = formWithPlugins({
    document,
    current: ['git+file:///opt/bar'],
  });

  const p = form.removePluginUrl('plugins', 0);
  assert.equal(dlg.open, true);
  assert.match(body.innerHTML, /<li class="font-mono">bar<\/li>/);
  cancelBtn.click();
  await p;
});

// ── confirmPluginRemoval fallback ────────────────────────────────────────────

test('confirmPluginRemoval fallback: window.confirm false keeps item', async () => {
  const form = formWithPlugins({
    current: ['github:madebydamo/neo-foo'],
    confirm: () => false,
  });
  // No dialog elements → fallback to window.confirm
  await form.removePluginUrl('plugins', 0);
  assert.deepEqual(form.values.plugins, ['github:madebydamo/neo-foo']);
});

test('confirmPluginRemoval fallback: window.confirm true removes item', async () => {
  const form = formWithPlugins({
    current: ['github:madebydamo/neo-foo'],
    confirm: () => true,
  });
  await form.removePluginUrl('plugins', 0);
  assert.deepEqual(form.values.plugins, []);
});

// ── plEmptyHint ──────────────────────────────────────────────────────────────

test('plEmptyHint: uses ui.emptyHint', () => {
  const form = formWithPlugins({ emptyHint: 'No plugins yet.' });
  assert.equal(form.plEmptyHint('plugins'), 'No plugins yet.');
});

test('plEmptyHint: falls back to long default when omitted', () => {
  const form = formWithPlugins({ emptyHint: null });
  assert.equal(
    form.plEmptyHint('plugins'),
    'No plugins yet. Add a flake URL (github:user/repo, git+file:/path, or path:/path).'
  );
});
