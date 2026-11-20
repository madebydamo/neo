'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { anchorY, curve, pathsThrough } = require('./access_map.js');

test('one anchor sits in the middle, several spread over the middle 60%', () => {
  const r = { top: 100, height: 50 };
  assert.equal(anchorY(r, 0, 1), 125);
  assert.equal(anchorY(r, 0, 3), 110);
  assert.equal(anchorY(r, 2, 3), 140);
});

test('curve is a horizontal S from start to end', () => {
  assert.equal(curve(0, 10, 100, 50), 'M0 10 C50 10 50 50 100 50');
  // Short gaps still bend.
  assert.equal(curve(0, 0, 20, 0), 'M0 0 C24 0 -4 0 20 0');
});

test('paths through a node go upstream and downstream', () => {
  const edges = [
    { from: 'o-web', to: 'd-web' },
    { from: 'o-local', to: 'd-web' },
    { from: 'd-web', to: 'g-open' },
    { from: 'd-web', to: 'g-guarded' },
    { from: 'o-local', to: 'd-ssh' },
  ];
  assert.deepEqual([...pathsThrough(edges, 'g-open')].sort(), [0, 1, 2]);
  assert.deepEqual([...pathsThrough(edges, 'o-local')].sort(), [1, 2, 3, 4]);
});
