'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { model } = require('./sidebar_layout.js');

const G = (id, items, extra) => Object.assign({ id: 'group:' + id, name: 'Group', open: true, items }, extra);

test('withServices: no layout keeps server order', () => {
  assert.deepEqual(model.withServices(null, ['a', 'b', 'c']), ['a', 'b', 'c']);
});

test('withServices: appends new services, keeps stored ones that are gone', () => {
  const stored = ['c', G('x', ['gone', 'a'])];
  assert.deepEqual(model.withServices(stored, ['a', 'b', 'c']), ['c', G('x', ['gone', 'a']), 'b']);
});

test('sanitize: drops junk, duplicates and empty groups', () => {
  const raw = ['a', 'a', 3, null, { id: 'nope', items: ['b'] }, G('e', []), G('y', ['a', 'b'], { name: ' ' })];
  assert.deepEqual(model.sanitize(raw), ['a', G('y', ['b'])]);
  assert.deepEqual(model.sanitize('garbage'), []);
});

test('move: reorder before / after', () => {
  assert.deepEqual(model.move(['a', 'b', 'c'], 'c', { type: 'before', id: 'a' }), ['c', 'a', 'b']);
  assert.deepEqual(model.move(['a', 'b', 'c'], 'a', { type: 'after', id: 'c' }), ['b', 'c', 'a']);
});

test('move: does not modify its input', () => {
  const t = ['a', 'b'];
  model.move(t, 'a', { type: 'after', id: 'b' });
  assert.deepEqual(t, ['a', 'b']);
});

test('move: merge onto a service creates a group in its place', () => {
  const out = model.move(['a', 'b', 'c'], 'c', { type: 'merge', id: 'a' });
  assert.equal(out.length, 2);
  assert.ok(model.isGroup(out[0]));
  assert.deepEqual(out[0].items, ['a', 'c']);
  assert.equal(out[0].open, true);
  assert.equal(out[1], 'b');
});

test('move: merge onto a group appends, onto a grouped service inserts after it', () => {
  const t = [G('x', ['a', 'b']), 'c', 'd'];
  assert.deepEqual(model.move(t, 'c', { type: 'merge', id: 'group:x' }), [G('x', ['a', 'b', 'c']), 'd']);
  assert.deepEqual(model.move(t, 'c', { type: 'merge', id: 'a' }), [G('x', ['a', 'c', 'b']), 'd']);
});

test('move: dragging the last service out removes the group', () => {
  const t = [G('x', ['a']), 'b'];
  assert.deepEqual(model.move(t, 'a', { type: 'after', id: 'b' }), ['b', 'a']);
});

test('move: groups do not nest', () => {
  const t = [G('x', ['a', 'b']), G('y', ['c']), 'd'];
  // Next to a grouped service → next to its group.
  assert.deepEqual(model.move(t, 'group:y', { type: 'before', id: 'b' }), [G('y', ['c']), G('x', ['a', 'b']), 'd']);
  // Merge with a group → plain reorder.
  assert.deepEqual(model.move(t, 'group:y', { type: 'merge', id: 'd' }), [G('x', ['a', 'b']), G('y', ['c']), 'd']);
  // Append to a group → top level.
  assert.deepEqual(model.move(t, 'group:y', { type: 'append', group: 'group:x' }), [G('x', ['a', 'b']), 'd', G('y', ['c'])]);
});

test('move: append to a group or the top level', () => {
  const t = [G('x', ['a']), 'b', 'c'];
  assert.deepEqual(model.move(t, 'b', { type: 'append', group: 'group:x' }), [G('x', ['a', 'b']), 'c']);
  assert.deepEqual(model.move(t, 'a', { type: 'append', group: null }), ['b', 'c', 'a']);
});

test('move: unknown ids and self targets are no-ops', () => {
  const t = ['a', 'b'];
  assert.equal(model.move(t, 'zz', { type: 'before', id: 'a' }), t);
  assert.equal(model.move(t, 'a', { type: 'merge', id: 'a' }), t);
  assert.equal(model.move(t, 'a', { type: 'before', id: 'zz' }), t);
});

test('rename / setOpen / ungroup', () => {
  const t = ['a', G('x', ['b', 'c']), 'd'];
  assert.equal(model.rename(t, 'group:x', '  Media  ')[1].name, 'Media');
  assert.equal(model.rename(t, 'group:x', '   ')[1].name, 'Group');
  assert.equal(model.setOpen(t, 'group:x', false)[1].open, false);
  assert.deepEqual(model.ungroup(t, 'group:x'), ['a', 'b', 'c', 'd']);
});
