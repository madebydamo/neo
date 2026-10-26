'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const L = require('./locks.js');

const activation = {
  label: 'Activation',
  message: 'Activation in progress (started 12:03)',
  scopes: [{ scope: 'system', mode: 'ex' }],
};
const restore = {
  label: 'Snapshot restore of calino',
  message: 'Snapshot restore of calino in progress (started 09:10)',
  scopes: [
    { scope: 'system', mode: 'sh' },
    { scope: 'service/calino', mode: 'ex' },
    { scope: 'unit/docker-calino.service', mode: 'ex' },
  ],
};

test('parseNeeds: modes and unit normalization', () => {
  assert.deepEqual(L.parseNeeds('system:sh unit/docker-x  unit/neo-x.target'), [
    { scope: 'system', mode: 'sh' },
    { scope: 'unit/docker-x.service', mode: 'ex' },
    { scope: 'unit/neo-x.target', mode: 'ex' },
  ]);
  assert.deepEqual(L.parseNeeds(''), []);
});

test('blocker: readers-writer semantics', () => {
  // Activation (system ex) blocks everything that touches system.
  assert.equal(L.blocker(L.parseNeeds('system'), [activation]), activation);
  assert.equal(L.blocker(L.parseNeeds('system:sh service/a'), [activation]), activation);
  // A restore holds system shared: other service ops fine, activation blocked.
  assert.equal(L.blocker(L.parseNeeds('system:sh service/other'), [restore]), null);
  assert.equal(L.blocker(L.parseNeeds('system'), [restore]), restore);
  // Its units are blocked, others are not.
  assert.equal(L.blocker(L.parseNeeds('unit/docker-calino'), [restore]), restore);
  assert.equal(L.blocker(L.parseNeeds('unit/docker-other'), [restore]), null);
  assert.equal(L.blocker(L.parseNeeds('service/calino:sh'), [restore]), restore);
  assert.equal(L.blocker(L.parseNeeds('unit/x'), []), null);
});

test('conflictMessage: only 409', () => {
  assert.equal(L.conflictMessage(409, ' Blocked: X in progress \n'), 'Blocked: X in progress');
  assert.equal(L.conflictMessage(409, ''), 'Blocked: another operation is running');
  assert.equal(L.conflictMessage(500, 'boom'), null);
});

// Minimal DOM stand-in for apply().
function el(attrs) {
  const a = Object.assign({}, attrs);
  const classes = new Set();
  return {
    disabled: false,
    classList: { add: (c) => classes.add(c), remove: (c) => classes.delete(c), has: (c) => classes.has(c), contains: (c) => classes.has(c) },
    getAttribute: (k) => (k in a ? a[k] : null),
    setAttribute: (k, v) => { a[k] = String(v); },
    removeAttribute: (k) => { delete a[k]; },
    hasAttribute: (k) => k in a,
  };
}

function doc(holders, els) {
  const locks = el({ 'data-locks': JSON.stringify(holders) });
  return {
    getElementById: (id) => (id === 'neo-locks' ? locks : null),
    querySelectorAll: () => els,
    locks,
  };
}

test('apply: disables with reason and restores title/disabled state', () => {
  const activate = el({ 'data-neo-lock': 'system', title: 'Build and switch' });
  const start = el({ 'data-neo-lock': 'unit/docker-calino' });
  const already = el({ 'data-neo-lock': 'system' });
  already.disabled = true;
  const d = doc([activation], [activate, start, already]);

  L.apply(d);
  assert.equal(activate.disabled, true);
  assert.equal(activate.getAttribute('title'), 'Blocked: Activation in progress (started 12:03)');
  assert.ok(activate.classList.has('btn-disabled'));
  assert.equal(start.disabled, false);

  d.locks.setAttribute('data-locks', '[]');
  L.apply(d);
  assert.equal(activate.disabled, false);
  assert.equal(activate.getAttribute('title'), 'Build and switch');
  assert.ok(!activate.classList.has('btn-disabled'));
  assert.ok(!activate.hasAttribute('data-neo-locked'));
  // Was disabled before the lock: stays disabled.
  assert.equal(already.disabled, true);
});

test('apply: in-flight htmx disable does not leave the grey class behind', () => {
  // hx-disabled-elt disabled the button when the lock arrived; htmx re-enables it later.
  const b = el({ 'data-neo-lock': 'system' });
  b.disabled = true;
  const d = doc([activation], [b]);
  L.apply(d);
  assert.ok(b.classList.has('btn-disabled'));
  b.disabled = false;
  d.locks.setAttribute('data-locks', '[]');
  L.apply(d);
  assert.equal(b.disabled, false);
  assert.ok(!b.classList.has('btn-disabled'));
});

test('apply: bad JSON means no holders', () => {
  const b = el({ 'data-neo-lock': 'system' });
  const d = doc([], [b]);
  d.locks.setAttribute('data-locks', '{not json');
  L.apply(d);
  assert.equal(b.disabled, false);
});
