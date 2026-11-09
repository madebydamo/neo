'use strict';
const test = require('node:test');
const assert = require('node:assert');
const NeoUtil = require('./neo_util.js');

test('esc escapes markup and treats null as empty', () => {
  assert.strictEqual(NeoUtil.esc(`<a href="x" title='y'>&`), '&lt;a href=&quot;x&quot; title=&#39;y&#39;&gt;&amp;');
  assert.strictEqual(NeoUtil.esc(null), '');
  assert.strictEqual(NeoUtil.esc(undefined), '');
  assert.strictEqual(NeoUtil.esc(0), '0');
});

test('toast does nothing without a host', () => {
  globalThis.document = { getElementById: () => null };
  try {
    NeoUtil.toast('hello', 'error');
  } finally {
    delete globalThis.document;
  }
});

function fakeWin() {
  const data = new Map();
  return {
    localStorage: {
      getItem: (k) => (data.has(k) ? data.get(k) : null),
      setItem: (k, v) => data.set(k, v),
      removeItem: (k) => data.delete(k),
    },
  };
}

test('store round-trips text and JSON', () => {
  const s = NeoUtil.store('local', fakeWin());
  assert.strictEqual(s.get('a'), null);
  s.set('a', '1');
  assert.strictEqual(s.get('a'), '1');
  s.remove('a');
  assert.strictEqual(s.get('a'), null);
  assert.deepStrictEqual(s.getJson('j', { none: true }), { none: true });
  s.setJson('j', { x: 1 });
  assert.deepStrictEqual(s.getJson('j', null), { x: 1 });
  s.set('bad', '{');
  assert.strictEqual(s.getJson('bad', 'fallback'), 'fallback');
});

test('store survives storage that throws', () => {
  const boom = () => {
    throw new Error('blocked');
  };
  const s = NeoUtil.store('local', { localStorage: { getItem: boom, setItem: boom, removeItem: boom } });
  assert.strictEqual(s.get('a'), null);
  assert.strictEqual(s.getJson('a', 7), 7);
  s.set('a', '1');
  s.setJson('a', 1);
  s.remove('a');
  // No storage at all (access itself fails).
  const none = NeoUtil.store('session', {});
  assert.strictEqual(none.get('a'), null);
  none.set('a', '1');
});

test('onReady waits for a loading document and runs at once otherwise', () => {
  let ran = 0;
  let listener = null;
  NeoUtil.onReady({ readyState: 'loading', addEventListener: (ev, fn) => { assert.strictEqual(ev, 'DOMContentLoaded'); listener = fn; } }, () => ran++);
  assert.strictEqual(ran, 0);
  listener();
  assert.strictEqual(ran, 1);
  NeoUtil.onReady({ readyState: 'complete' }, () => ran++);
  assert.strictEqual(ran, 2);
});

test('reloadPane loads the pane URL into the content area', () => {
  const calls = [];
  globalThis.htmx = { ajax: (...args) => calls.push(args) };
  globalThis.document = { getElementById: () => ({ dataset: { loadUrl: '/configuration/option/a' } }) };
  try {
    assert.strictEqual(NeoUtil.reloadPane(), true);
    assert.deepStrictEqual(calls, [['GET', '/configuration/option/a', { target: '#config-content', swap: 'innerHTML' }]]);
    globalThis.document = { getElementById: () => null };
    assert.strictEqual(NeoUtil.reloadPane(), false);
  } finally {
    delete globalThis.htmx;
    delete globalThis.document;
  }
});

test('readJson returns the body and rejects with the best message', async () => {
  const res = (ok, status, json) => ({ ok, status, json });
  assert.deepStrictEqual(await NeoUtil.readJson(res(true, 200, async () => ({ a: 1 }))), { a: 1 });
  await assert.rejects(NeoUtil.readJson(res(false, 500, async () => ({ error: 'nope' }))), /^Error: nope$/);
  await assert.rejects(NeoUtil.readJson(res(false, 500, async () => ({})), 'Broken'), /^Error: Broken \(500\)\.$/);
  const notJson = async () => {
    throw new SyntaxError('Unexpected token <');
  };
  await assert.rejects(NeoUtil.readJson(res(false, 502, notJson)), /^Error: Request failed \(502\)\.$/);
  await assert.rejects(NeoUtil.readJson(res(true, 200, notJson), 'Broken'), /^Error: Broken \(200\)\.$/);
});
