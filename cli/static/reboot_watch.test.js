'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const W = require('./reboot_watch.js');

const ok = (text) => ({ ok: true, status: 200, type: 'basic', text });

test('verdict: same boot keeps waiting, a new boot reloads', () => {
  assert.equal(W.verdict('a', ok('a\n'), false), 'wait');
  assert.equal(W.verdict('a', ok('a'), true), 'wait');
  assert.equal(W.verdict('a', ok('b\n'), true), 'reload');
  // Even without having seen it down (the poll missed the gap).
  assert.equal(W.verdict('a', ok('b'), false), 'reload');
});

test('verdict: down or erroring server waits', () => {
  assert.equal(W.verdict('a', null, true), 'wait');
  assert.equal(W.verdict('a', { ok: false, status: 502, type: 'basic' }, true), 'wait');
  assert.equal(W.verdict('a', ok(''), true), 'wait');
});

test('verdict: auth redirect reloads only after the server was gone', () => {
  const redirect = { ok: false, status: 0, type: 'opaqueredirect' };
  assert.equal(W.verdict('a', redirect, false), 'wait');
  assert.equal(W.verdict('a', redirect, true), 'reload');
  assert.equal(W.verdict('a', { ok: false, status: 401, type: 'basic' }, true), 'reload');
});

test('formatElapsed: minutes and zero-padded seconds', () => {
  assert.equal(W.formatElapsed(0), '0:00');
  assert.equal(W.formatElapsed(9999), '0:09');
  assert.equal(W.formatElapsed(75000), '1:15');
  assert.equal(W.formatElapsed(-5), '0:00');
});
