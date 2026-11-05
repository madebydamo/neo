'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const P = require('./proposal_card.js');

const fill = (b) => crypto.getRandomValues(b);

test('generate: 24 alphanumeric characters by default', () => {
  const s = P.generate(undefined, fill);
  assert.equal(s.length, 24);
  assert.match(s, /^[A-Za-z0-9]+$/);
  assert.notEqual(P.generate(24, fill), s);
  assert.equal(P.generate(40, fill).length, 40);
});

test('generate: values above the sampling limit are skipped', () => {
  let round = 0;
  // First round: all rejected (max uint32), then all zeros → "A".
  const s = P.generate(3, (b) => {
    b.fill(round++ === 0 ? 0xffffffff : 0);
    return b;
  });
  assert.equal(s, 'AAA');
});

test('missing: empty and blank inputs', () => {
  const inputs = [{ value: 'pw' }, { value: '' }, { value: '   ' }];
  assert.deepEqual(P.missing(inputs), [inputs[1], inputs[2]]);
  assert.deepEqual(P.missing([]), []);
});
