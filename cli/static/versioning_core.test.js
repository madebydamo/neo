'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const VC = require('./versioning_core.js');

test('prevActivation walks first parents to the nearest activation', () => {
  const commits = {
    c3: { id: 'c3', kind: 'commit', parents: ['c2'] },
    c2: { id: 'c2', kind: 'commit', parents: ['c1'] },
    c1: { id: 'c1', kind: 'activation', parents: [] },
  };
  const byId = (id) => commits[id] || null;
  assert.equal(VC.prevActivation({ parents: ['c3'] }, byId), 'c1');
  assert.equal(VC.prevActivation({ parents: ['zz'] }, byId), 'zz');
  assert.equal(VC.prevActivation({ parents: [] }, byId), null);
  assert.equal(VC.prevActivation({}, byId), null);
});

test('commitTitle: changes first, else the kind, else the cleaned subject', () => {
  assert.equal(VC.commitTitle({ kind: 'activation', changes: { unchanged: true } }), 'Activated again · no settings changes');
  assert.equal(VC.commitTitle({ kind: 'activation' }), 'Activation');
  assert.equal(VC.commitTitle({ kind: 'build' }), 'Build');
  assert.equal(VC.commitTitle({ kind: 'commit', subject: 'neo: activation_20260101-120000 ok' }), 'neo:  ok');
  assert.equal(VC.commitTitle({ kind: 'commit', subject: 'activation_20260101-120000' }), 'Commit');
  assert.equal(VC.commitKindLabel({ kind: 'x' }), 'Commit');
  assert.equal(VC.commitKindLabel({ kind: 'build' }), 'Build');
});

test('linked and system titles', () => {
  assert.equal(VC.linked({ commit: {}, system: { type: 'generation' } }), true);
  assert.equal(VC.linked({ commit: {}, system: { type: 'switch' } }), false);
  assert.equal(VC.linked({ system: { type: 'generation' } }), false);
  assert.equal(VC.sysTitle({ type: 'switch', to: 4, rollback: true }), 'Rolled back to generation 4');
  assert.equal(VC.sysTitle({ type: 'switch', to: 4 }), 'Switched to generation 4');
  assert.equal(VC.sysTitle({ type: 'generation', number: 9 }), 'Generation 9');
});

test('sysSubtitle explains where a generation or switch comes from', () => {
  assert.equal(VC.sysSubtitle({ system: { type: 'switch', from: 3 } }), 'from generation 3 · builds continue from here');
  assert.equal(VC.sysSubtitle({ system: { type: 'switch' } }), 'builds continue from here');
  assert.equal(VC.sysSubtitle({ commit: {}, system: { type: 'generation' } }), 'New system');
  assert.equal(VC.sysSubtitle({ commit: {}, system: { type: 'generation', identicalTo: 2 } }), 'Identical to generation 2');
  assert.equal(
    VC.sysSubtitle({ system: { type: 'generation', branched: true, parentGen: 5, builtFrom: { short: 'abc', how: 'inferred' } } }),
    'Built on generation 5 after a rollback · from version abc (likely)',
  );
  assert.equal(VC.sysSubtitle({ system: { type: 'generation' } }), 'not linked to a saved version');
});

test('genStateText per generation state', () => {
  assert.equal(VC.genStateText({ kind: 'activation' }, true), 'No system recorded');
  assert.equal(VC.genStateText({ kind: 'activation' }, false), '');
  assert.equal(VC.genStateText({ kind: 'commit' }, true), '');
  assert.equal(VC.genStateText({ generation: 3, genState: 'same' }), 'Same system as generation 3');
  assert.equal(VC.genStateText({ generation: 3, genState: 'missing' }), 'Generation 3 · deleted');
  assert.equal(VC.genStateText({ generation: 3, genState: 'created' }), 'Built generation 3');
  assert.equal(VC.genStateText({ generation: 3 }), 'Generation 3');
});

test('hoverLinks finds the linked commit or generation', () => {
  assert.deepEqual(VC.hoverLinks({ system: { type: 'generation', builtFrom: { id: 'c1' } } }), { commit: 'c1', gen: null });
  assert.deepEqual(VC.hoverLinks({ system: { type: 'switch', to: 7 } }), { commit: null, gen: 7 });
  assert.deepEqual(VC.hoverLinks({ commit: { generation: 4, genState: 'same' } }), { commit: null, gen: 4 });
  assert.deepEqual(VC.hoverLinks({ commit: { isRunningConfig: true } }, 9), { commit: null, gen: 9 });
  assert.equal(VC.hoverLinks({ commit: { isRunningConfig: true } }, undefined), null);
  assert.equal(VC.hoverLinks({ commit: {} }), null);
});

test('rowClass: open, highlighted, compare dimming and picks', () => {
  const row = { key: 'k', commit: { id: 'c1' }, system: { type: 'generation', number: 2 } };
  assert.equal(VC.rowClass(row, { expanded: 'k', compare: false, hl: null, picks: [] }), 'vt-open');
  assert.equal(VC.rowClass(row, { expanded: 'k', compare: true, hl: null, picks: ['c1'] }), 'vt-picked');
  assert.equal(VC.rowClass(row, { expanded: null, compare: false, hl: { gen: 2 }, picks: [] }), 'vt-hl');
  assert.equal(VC.rowClass(row, { expanded: null, compare: false, hl: { commit: 'c1' }, picks: [] }), 'vt-hl');
  assert.equal(VC.rowClass({ key: 'g' }, { expanded: null, compare: true, hl: null, picks: [] }), 'opacity-40');
});

test('rowTime uses the commit, switch or creation time', () => {
  assert.equal(VC.rowTime({ commit: { timestamp: 5 } }), 5);
  assert.equal(VC.rowTime({ system: { type: 'switch', time: 6 } }), 6);
  assert.equal(VC.rowTime({ system: { type: 'generation', created: 7 } }), 7);
  assert.equal(VC.rowTime({}), 0);
});

test('togglePick keeps at most two picks and replaces B', () => {
  const p = [];
  VC.togglePick(p, 'a');
  VC.togglePick(p, 'b');
  assert.deepEqual(p, ['a', 'b']);
  VC.togglePick(p, 'c');
  assert.deepEqual(p, ['a', 'c']);
  VC.togglePick(p, 'a');
  assert.deepEqual(p, ['c']);
});

test('dayLabel says Today and Yesterday', () => {
  const now = new Date(2026, 5, 15, 14, 0, 0).getTime();
  assert.equal(VC.dayLabel(new Date(2026, 5, 15, 1, 0, 0).getTime() / 1000, now), 'Today');
  assert.equal(VC.dayLabel(new Date(2026, 5, 14, 23, 0, 0).getTime() / 1000, now), 'Yesterday');
  assert.notEqual(VC.dayLabel(new Date(2026, 5, 10).getTime() / 1000, now), 'Today');
  assert.match(VC.dayLabel(new Date(2024, 0, 3).getTime() / 1000, now), /2024/);
});
