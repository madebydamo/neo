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
