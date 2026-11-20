'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { tagsMatch, applyFilter } = require('./access_filter.js');

test('"all" and an empty filter match every row', () => {
  assert.equal(tagsMatch('', 'all'), true);
  assert.equal(tagsMatch('internet', ''), true);
});

test('a filter matches whole tags only', () => {
  assert.equal(tagsMatch('internet nologin internet-nologin', 'internet-nologin'), true);
  assert.equal(tagsMatch('internet nologin', 'internet-nologin'), false);
  assert.equal(tagsMatch('internet-nologin', 'internet'), false);
});

function fakeRow(tags, section, mode) {
  const classes = new Set();
  return {
    hidden: false,
    classList: { toggle: (c, on) => (on ? classes.add(c) : classes.delete(c)), has: (c) => classes.has(c) },
    getAttribute: (n) =>
      ({ 'data-neo-tags': tags, 'data-neo-section': section, 'data-neo-filter-mode': mode || null })[n] ?? null,
  };
}

test('apps hide, ports dim, and empty notes show per section', () => {
  const app = fakeRow('internet', 'apps');
  const appOk = fakeRow('internet nologin internet-nologin', 'apps');
  const port = fakeRow('internet skipsfw', 'ports', 'dim');
  const note = { hidden: false, getAttribute: () => 'ports' };
  const el = {
    querySelectorAll: (sel) => (sel === '[data-neo-tags]' ? [app, appOk, port] : [note]),
  };
  const found = applyFilter(el, 'internet-nologin');
  assert.deepEqual(found, { apps: 1 });
  assert.equal(app.hidden, true);
  assert.equal(appOk.hidden, false);
  assert.equal(port.hidden, false);
  assert.equal(port.classList.has('opacity-15'), true);
  assert.equal(note.hidden, false);
  applyFilter(el, 'all');
  assert.equal(app.hidden, false);
  assert.equal(port.classList.has('opacity-15'), false);
  assert.equal(note.hidden, true);
});
