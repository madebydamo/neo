'use strict';

const { test, beforeEach } = require('node:test');
const assert = require('node:assert/strict');
const path = require('path');

const registryPath = path.join(__dirname, '..', 'registry.js');

function loadFreshRegistry() {
  delete require.cache[require.resolve(registryPath)];
  const NeoWidgets = require(registryPath);
  NeoWidgets.resetForTests();
  return NeoWidgets;
}

beforeEach(() => {
  loadFreshRegistry();
});

test('register stores a widget by ui.widget name', () => {
  const NeoWidgets = globalThis.NeoWidgets;
  const def = { mixins: { foo() { return 1; } } };
  NeoWidgets.register('exclusiveListPair', def);
  assert.equal(NeoWidgets.get('exclusiveListPair'), def);
  assert.deepEqual(NeoWidgets.names(), ['exclusiveListPair']);
});

test('get returns undefined for unknown or empty names', () => {
  const NeoWidgets = globalThis.NeoWidgets;
  assert.equal(NeoWidgets.get('nope'), undefined);
  assert.equal(NeoWidgets.get(''), undefined);
  assert.equal(NeoWidgets.get(null), undefined);
});

test('register rejects a missing name or definition', () => {
  const NeoWidgets = globalThis.NeoWidgets;
  assert.throws(() => NeoWidgets.register('', { mixins: {} }), /name is required/);
  assert.throws(() => NeoWidgets.register('x', null), /definition is required/);
});

test('mixins merges methods from every registered widget', () => {
  const NeoWidgets = globalThis.NeoWidgets;
  NeoWidgets.register('a', { mixins: { alpha() { return 'A'; } } });
  NeoWidgets.register('b', { mixins: { beta() { return 'B'; } } });
  const mixed = NeoWidgets.mixins();
  assert.equal(mixed.alpha(), 'A');
  assert.equal(mixed.beta(), 'B');
});

test('later register of the same name replaces the previous widget', () => {
  const NeoWidgets = globalThis.NeoWidgets;
  NeoWidgets.register('w', { mixins: { n() { return 1; } } });
  NeoWidgets.register('w', { mixins: { n() { return 2; } } });
  assert.equal(NeoWidgets.mixins().n(), 2);
});
