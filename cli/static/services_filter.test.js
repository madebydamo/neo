'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { serviceMatches, cardFromDataset } = require('./services_filter.js');

function card(overrides) {
  return Object.assign(
    { enabled: false, cat: 'media', pluginIds: [], search: 'jellyfin media stream video' },
    overrides
  );
}

test('status "all" matches installed and available cards', () => {
  assert.equal(serviceMatches(card({ enabled: true }), { status: 'all' }), true);
  assert.equal(serviceMatches(card({ enabled: false }), { status: 'all' }), true);
});

test('status "installed" only matches enabled cards', () => {
  assert.equal(serviceMatches(card({ enabled: true }), { status: 'installed' }), true);
  assert.equal(serviceMatches(card({ enabled: false }), { status: 'installed' }), false);
});

test('status "available" only matches disabled cards', () => {
  assert.equal(serviceMatches(card({ enabled: false }), { status: 'available' }), true);
  assert.equal(serviceMatches(card({ enabled: true }), { status: 'available' }), false);
});

test('category filter restricts to the matching category', () => {
  const c = card({ cat: 'media' });
  assert.equal(serviceMatches(c, { category: 'media' }), true);
  assert.equal(serviceMatches(c, { category: 'network' }), false);
  assert.equal(serviceMatches(c, { category: 'all' }), true);
});

test('plugin filter: "core" matches cards with no plugin urls', () => {
  assert.equal(serviceMatches(card({ pluginIds: [] }), { plugin: 'core' }), true);
  assert.equal(serviceMatches(card({ pluginIds: ['github:foo/bar'] }), { plugin: 'core' }), false);
});

test('plugin filter: a plugin url only matches cards carrying it', () => {
  const c = card({ pluginIds: ['github:foo/bar', 'github:foo/baz'] });
  assert.equal(serviceMatches(c, { plugin: 'github:foo/bar' }), true);
  assert.equal(serviceMatches(c, { plugin: 'github:other/one' }), false);
});

test('search matches on the precomputed data-search haystack, case-insensitively, all terms required', () => {
  const c = card({ search: 'jellyfin media stream video' });
  assert.equal(serviceMatches(c, { q: 'jelly' }), true);
  assert.equal(serviceMatches(c, { q: 'JELLY STREAM' }), true);
  assert.equal(serviceMatches(c, { q: 'jelly nope' }), false);
  assert.equal(serviceMatches(c, { q: '  ' }), true);
});

test('all filters combine (AND)', () => {
  const c = card({ enabled: true, cat: 'media', pluginIds: [], search: 'jellyfin media' });
  assert.equal(
    serviceMatches(c, { status: 'installed', category: 'media', plugin: 'core', q: 'jelly' }),
    true
  );
  assert.equal(
    serviceMatches(c, { status: 'available', category: 'media', plugin: 'core', q: 'jelly' }),
    false
  );
});

test('cardFromDataset adapts a DOMStringMap-like object', () => {
  const c = cardFromDataset({
    enabled: 'true',
    cat: 'media',
    pluginIds: 'github:a/b|github:c/d',
    search: 'jellyfin media',
  });
  assert.deepEqual(c, {
    enabled: true,
    cat: 'media',
    pluginIds: ['github:a/b', 'github:c/d'],
    search: 'jellyfin media',
  });
});

test('cardFromDataset defaults missing fields safely', () => {
  const c = cardFromDataset({});
  assert.deepEqual(c, { enabled: false, cat: undefined, pluginIds: [], search: '' });
  assert.equal(serviceMatches(c, {}), true);
});
