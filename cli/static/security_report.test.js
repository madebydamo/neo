'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const R = require('./security_report.js');

test('stateAfter: busy while a check runs or waits to start', () => {
  assert.equal(R.stateAfter({ running: true, requested: false, finished_at: 5 }, '5'), 'busy');
  assert.equal(R.stateAfter({ running: false, requested: true, finished_at: 5 }, '5'), 'busy');
});

test('stateAfter: a new report offers a reload, the same one goes idle', () => {
  assert.equal(R.stateAfter({ running: false, requested: false, finished_at: 7 }, '5'), 'done');
  assert.equal(R.stateAfter({ running: false, requested: false, finished_at: 5 }, '5'), 'idle');
  // First check on an empty page (rendered with finished_at 0).
  assert.equal(R.stateAfter({ running: false, requested: false, finished_at: 9 }, '0'), 'done');
});
