'use strict';

require('./registry.js');
require('./plugin_list.js');
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { makeForm, createDocument } = require('./test/harness.js');

const inventory = [
  { id: 'neo-foo', url: 'github:madebydamo/neo-foo', prefix: '', services: ['foo', 'foo-worker'] },
  { id: 'bar', url: 'git+file:///opt/bar', prefix: '', services: ['bar'] },
  { id: 'neo-shared', url: 'github:madebydamo/neo-shared', prefix: '', services: ['foo'] },
  { id: 'portrait-dev', url: 'github:me/portrait/dev', prefix: 'dev', services: ['dev-portrait'] },
];

function pluginsOption(current) {
  return {
    name: 'plugins',
    type: { kind: 'attrsOf', elem: { kind: 'submodule' } },
    default: {},
    current: current || {},
    ui: { widget: 'pluginList', emptyHint: 'No plugins yet.' },
  };
}

function formWithPlugins(opts) {
  opts = opts || {};
  const current = opts.current !== undefined ? opts.current : {};
  const option = pluginsOption(current);
  if (opts.emptyHint === null) delete option.ui.emptyHint;
  const docOpts = {
    pluginInventory: opts.pluginInventory !== undefined ? opts.pluginInventory : inventory,
    confirm: opts.confirm,
  };
  if (opts.document) docOpts.document = opts.document;
  const form = makeForm({ ...docOpts, options: [option] });
  form.initWidgets();
  // initForm recaptures originals after widget init.
  form.originals.plugins = form.cloneValue(form.values.plugins);
  return form;
}

const one = (url, prefix) => ({ url, prefix: prefix || '' });

// ── URL helpers ──────────────────────────────────────────────────────────────

test('pluginLabel: repository name for forges, last segment otherwise', () => {
  const form = formWithPlugins();
  assert.equal(form.pluginLabel('github:user/repo'), 'repo');
  assert.equal(form.pluginLabel('github:user/repo/dev'), 'repo');
  assert.equal(form.pluginLabel('git+https://host/a/b.git?ref=x'), 'b');
  assert.equal(form.pluginLabel('path:/abs/plugin/'), 'plugin');
  assert.equal(form.pluginLabel(''), '');
});

test('plSanitize matches utils::plugins::sanitize_id', () => {
  const form = formWithPlugins();
  assert.equal(form.plSanitize('highsea.neo'), 'highsea-neo');
  assert.equal(form.plSanitize('bielCrawler'), 'bielcrawler');
  assert.equal(form.plSanitize('3d'), 'plugin-3d');
  assert.equal(form.plSanitize('...'), 'plugin');
});

test('plRefOf / plWithRef: forge path refs, ?ref= elsewhere, none for path:', () => {
  const form = formWithPlugins();
  assert.equal(form.plRefOf('github:me/portrait'), '');
  assert.equal(form.plRefOf('github:me/portrait/dev'), 'dev');
  assert.equal(form.plRefOf('github:me/portrait?ref=feature/x'), 'feature/x');
  assert.equal(form.plRefOf('git+ssh://git@host/me/p?ref=dev'), 'dev');
  assert.equal(form.plRefOf('git+file:/home/me/p'), '');
  assert.equal(form.plRefOf('path:/home/me/p'), null);

  assert.equal(form.plWithRef('github:me/portrait', 'dev'), 'github:me/portrait/dev');
  assert.equal(form.plWithRef('github:me/portrait/dev', ''), 'github:me/portrait');
  assert.equal(form.plWithRef('github:me/portrait/dev', 'main'), 'github:me/portrait/main');
  assert.equal(form.plWithRef('github:me/portrait?dir=x', 'feature/y'), 'github:me/portrait?dir=x&ref=feature%2Fy');
  assert.equal(form.plWithRef('git+file:/home/me/p', 'dev'), 'git+file:/home/me/p?ref=dev');
  assert.equal(form.plWithRef('git+file:/home/me/p?ref=dev&rev=1', ''), 'git+file:/home/me/p?rev=1');
  assert.equal(form.plWithRef('path:/home/me/p', 'dev'), 'path:/home/me/p');
});

test('plRepoKey ignores branches', () => {
  const form = formWithPlugins();
  assert.equal(form.plRepoKey('github:Me/Portrait/dev'), form.plRepoKey('github:me/portrait'));
  assert.equal(form.plRepoKey('git+file:///h/p?ref=x'), form.plRepoKey('git+file:/h/p'));
});

// ── init ─────────────────────────────────────────────────────────────────────

test('init: a legacy list becomes id-keyed entries', () => {
  const form = formWithPlugins({ current: ['github:a/portrait', 'github:b/highsea.neo', 'path:/x/portrait'] });
  assert.deepEqual(form.values.plugins, {
    'portrait-0': one('github:a/portrait'),
    'highsea-neo': one('github:b/highsea.neo'),
    'portrait-2': one('path:/x/portrait'),
  });
});

test('init: tables keep their order and fill prefix', () => {
  const form = formWithPlugins({ current: { b: { url: 'u2' }, a: { url: 'u1', prefix: 'x' } } });
  assert.deepEqual(form.plIds('plugins'), ['b', 'a']);
  assert.deepEqual(form.values.plugins.b, one('u2'));
});

// ── add ──────────────────────────────────────────────────────────────────────

test('addPluginUrl: id from the repository name, draft cleared', () => {
  const form = formWithPlugins();
  form.plDraft.plugins = '  github:madebydamo/highsea.neo ';
  form.addPluginUrl('plugins');
  assert.deepEqual(form.values.plugins, { 'highsea-neo': one('github:madebydamo/highsea.neo') });
  assert.equal(form.plDraft.plugins, '');
});

test('addPluginUrl: empty draft is a no-op', () => {
  const form = formWithPlugins();
  form.addPluginUrl('plugins');
  assert.deepEqual(form.values.plugins, {});
  assert.equal(form.plError.plugins, '');
});

test('addPluginUrl: same plugin again needs a different prefix', () => {
  const form = formWithPlugins({ current: { portrait: one('github:me/portrait') } });
  form.plDraft.plugins = 'github:me/portrait/dev';
  form.addPluginUrl('plugins');
  assert.match(form.plError.plugins, /already added/);
  assert.deepEqual(form.plIds('plugins'), ['portrait']);

  form.plDraftPrefix.plugins = 'dev';
  form.addPluginUrl('plugins');
  assert.deepEqual(form.values.plugins['portrait-dev'], one('github:me/portrait/dev', 'dev'));
});

test('addPluginUrl: rejects prefixes with other characters', () => {
  const form = formWithPlugins();
  form.plDraft.plugins = 'github:me/portrait';
  form.plDraftPrefix.plugins = 'Dev-';
  form.addPluginUrl('plugins');
  assert.match(form.plError.plugins, /lowercase/);
});

test('plSuggestPrefix: branch name when the plugin is already there', () => {
  const form = formWithPlugins({ current: { portrait: one('github:me/portrait') } });
  form.plDraft.plugins = 'github:me/portrait/Feature_1';
  form.plSuggestPrefix('plugins');
  assert.equal(form.plDraftPrefix.plugins, 'feature1');
  assert.equal(form.plNeedsPrefix('plugins'), true);
});

test('plSuggestPrefix: no branch → copy2; new repo → nothing', () => {
  const form = formWithPlugins({ current: { portrait: one('github:me/portrait') } });
  form.plDraft.plugins = 'github:me/portrait';
  form.plSuggestPrefix('plugins');
  assert.equal(form.plDraftPrefix.plugins, 'copy2');

  const fresh = formWithPlugins();
  fresh.plDraft.plugins = 'github:me/portrait/dev';
  fresh.plSuggestPrefix('plugins');
  assert.equal(fresh.plDraftPrefix.plugins, '');
});

test('plSuggestPrefix: follows the URL while typing, stops once the user types a prefix', () => {
  const form = formWithPlugins({ current: { portrait: one('git+file:/p/portrait') } });
  for (const url of ['git+file:/p/portrait', 'git+file:/p/portrait?ref=fe', 'git+file:/p/portrait?ref=feature']) {
    form.plDraft.plugins = url;
    form.plSuggestPrefix('plugins');
  }
  assert.equal(form.plDraftPrefix.plugins, 'feature');
  form.plDraft.plugins = 'git+file:/other';
  form.plSuggestPrefix('plugins');
  assert.equal(form.plDraftPrefix.plugins, '');

  form.plDraft.plugins = 'git+file:/p/portrait?ref=x';
  form.plDraftPrefix.plugins = 'mine';
  form.plPrefixTyped('plugins');
  form.plSuggestPrefix('plugins');
  assert.equal(form.plDraftPrefix.plugins, 'mine');
});

test('plSanitizePrefix: starts with a letter', () => {
  const form = formWithPlugins();
  assert.equal(form.plSanitizePrefix('2nd-Try'), 'ndtry');
  assert.equal(form.plSanitizePrefix('v2'), 'v2');
});

test('plNewId: unique ids', () => {
  const form = formWithPlugins({ current: { portrait: one('github:a/portrait') } });
  assert.equal(form.plNewId('plugins', 'github:b/portrait', ''), 'portrait-2');
  assert.equal(form.plNewId('plugins', 'github:b/portrait', 'dev'), 'portrait-dev');
});

// ── edit ─────────────────────────────────────────────────────────────────────

test('edit: changing the branch rewrites only that url and keeps the id', () => {
  const form = formWithPlugins({
    current: { a: one('github:me/a'), portrait: one('github:me/portrait') },
  });
  form.startPluginEdit('plugins', 'portrait');
  assert.equal(form.plEdit.plugins.ref, '');
  form.plEdit.plugins.ref = 'dev';
  form.plEditBranchChanged('plugins');
  assert.equal(form.plEdit.plugins.url, 'github:me/portrait/dev');
  form.applyPluginEdit('plugins');
  assert.deepEqual(form.values.plugins, {
    a: one('github:me/a'),
    portrait: one('github:me/portrait/dev'),
  });
  assert.equal(form.plEdit.plugins, null);
});

test('edit: typing a url updates the branch field', () => {
  const form = formWithPlugins({ current: { p: one('github:me/p') } });
  form.startPluginEdit('plugins', 'p');
  form.plEdit.plugins.url = 'git+file:/x/p?ref=wip';
  form.plEditUrlChanged('plugins');
  assert.equal(form.plEdit.plugins.ref, 'wip');
});

test('edit: empty address or a clash with another instance is refused', () => {
  const form = formWithPlugins({
    current: { p: one('github:me/p'), q: one('github:me/q') },
  });
  form.startPluginEdit('plugins', 'q');
  form.plEdit.plugins.url = '';
  form.applyPluginEdit('plugins');
  assert.match(form.plError.plugins, /empty/);
  form.plEdit.plugins.url = 'github:me/p/dev';
  form.applyPluginEdit('plugins');
  assert.match(form.plError.plugins, /"p"/);
  assert.equal(form.values.plugins.q.url, 'github:me/q');
});

test('edit: cancel keeps the value', () => {
  const form = formWithPlugins({ current: { p: one('github:me/p') } });
  form.startPluginEdit('plugins', 'p');
  form.plEdit.plugins.url = 'github:me/other';
  form.cancelPluginEdit('plugins');
  assert.equal(form.values.plugins.p.url, 'github:me/p');
});

// ── save ─────────────────────────────────────────────────────────────────────

test('prepareSave: undefined when unchanged; empty prefix left out', () => {
  const w = require('./plugin_list.js');
  const form = formWithPlugins({ current: { p: one('github:me/p') } });
  assert.equal(w.prepareSave.call(form, 'plugins'), undefined);
  form.plDraft.plugins = 'github:me/p/dev';
  form.plDraftPrefix.plugins = 'dev';
  form.addPluginUrl('plugins');
  assert.deepEqual(w.prepareSave.call(form, 'plugins'), {
    p: { url: 'github:me/p' },
    'p-dev': { url: 'github:me/p/dev', prefix: 'dev' },
  });
});

// ── inventory / remove ───────────────────────────────────────────────────────

test('plServices: by instance id', () => {
  const form = formWithPlugins();
  assert.deepEqual(form.plServices('portrait-dev'), ['dev-portrait']);
  assert.deepEqual(form.plServices('nope'), []);
});

test('pluginRemovalPreviewFor: shared service kept while a co-owner remains', () => {
  const form = formWithPlugins({
    current: { 'neo-foo': one('github:madebydamo/neo-foo'), 'neo-shared': one('github:madebydamo/neo-shared') },
  });
  const [row] = form.pluginRemovalPreviewFor('plugins', ['neo-foo']);
  assert.deepEqual(row.services, ['foo-worker']);
  const both = form.pluginRemovalPreviewFor('plugins', ['neo-foo', 'neo-shared']);
  assert.deepEqual(both[0].services, ['foo', 'foo-worker']);
});

test('removePlugin: an entry added in this session goes without asking', async () => {
  const form = formWithPlugins({ confirm: () => { throw new Error('asked'); } });
  form.plDraft.plugins = 'github:me/p';
  form.addPluginUrl('plugins');
  await form.removePlugin('plugins', 'p');
  assert.deepEqual(form.values.plugins, {});
});

test('confirmPluginRemoval dialog: confirm removes; HTML-escaped', async () => {
  const document = createDocument({ pluginInventory: inventory });
  const dlg = document.set('plugin-remove-dialog-plugins');
  const body = document.set('plugin-remove-body-plugins');
  const okBtn = document.set('plugin-remove-confirm-plugins');
  document.set('plugin-remove-cancel-plugins');
  const form = formWithPlugins({ document, current: { 'portrait-dev': one('github:u/a&b<"x">', 'dev') } });

  const p = form.removePlugin('plugins', 'portrait-dev');
  assert.equal(dlg.open, true);
  assert.match(body.innerHTML, /github:u\/a&amp;b&lt;&quot;x&quot;&gt;/);
  assert.match(body.innerHTML, /<li class="font-mono">dev-portrait<\/li>/);
  okBtn.click();
  await p;
  assert.deepEqual(form.values.plugins, {});
});

test('confirmPluginRemoval dialog: cancel keeps the entry', async () => {
  const document = createDocument({ pluginInventory: inventory });
  document.set('plugin-remove-dialog-plugins');
  document.set('plugin-remove-body-plugins');
  document.set('plugin-remove-confirm-plugins');
  const cancelBtn = document.set('plugin-remove-cancel-plugins');
  const form = formWithPlugins({ document, current: { bar: one('git+file:///opt/bar') } });
  const p = form.removePlugin('plugins', 'bar');
  cancelBtn.click();
  await p;
  assert.deepEqual(Object.keys(form.values.plugins), ['bar']);
});

test('confirmPluginRemoval fallback: window.confirm decides', async () => {
  const keep = formWithPlugins({ current: { bar: one('x') }, confirm: () => false });
  await keep.removePlugin('plugins', 'bar');
  assert.deepEqual(Object.keys(keep.values.plugins), ['bar']);
  const drop = formWithPlugins({ current: { bar: one('x') }, confirm: () => true });
  await drop.removePlugin('plugins', 'bar');
  assert.deepEqual(drop.values.plugins, {});
});

// ── plEmptyHint ──────────────────────────────────────────────────────────────

test('plEmptyHint: ui.emptyHint, else the default', () => {
  assert.equal(formWithPlugins().plEmptyHint('plugins'), 'No plugins yet.');
  assert.match(formWithPlugins({ emptyHint: null }).plEmptyHint('plugins'), /flake URL/);
});
