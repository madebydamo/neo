'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const VT = require('./version_tree.js');

test('notes look up commits and generations', () => {
  const notes = { commits: { abc: 'known good' }, generations: { 7: 'fresh setup' } };
  assert.equal(VT.commitNote(notes, 'abc'), 'known good');
  assert.equal(VT.commitNote(notes, 'def'), '');
  assert.equal(VT.genNote(notes, 7), 'fresh setup');
  assert.equal(VT.genNote(notes, 8), '');
  assert.equal(VT.genNote(null, 7), '');
  assert.equal(VT.commitNote(undefined, 'abc'), '');
});

test('note form body is url-encoded', () => {
  assert.equal(VT.noteBody('generation', 7, 'a & b'), 'kind=generation&id=7&note=a+%26+b');
  assert.equal(VT.noteBody('commit', 'abc', null), 'kind=commit&id=abc&note=');
});

test('metrics shrink lanes to fit and vanish without lanes', () => {
  assert.deepEqual(VT.metrics(0), { pad: 0, lw: 0, width: 0 });
  const one = VT.metrics(1);
  assert.equal(one.lw, 15);
  assert.equal(one.width, 37);
  const many = VT.metrics(20);
  assert.equal(many.lw, 7); // floor
  const compact = VT.metrics(3, true);
  assert.ok(compact.width <= 64 + 1);
});

test('laneX mirrors for the system gutter', () => {
  const m = VT.metrics(2);
  assert.equal(VT.laneX(m, 0), m.pad + m.lw / 2);
  assert.equal(VT.laneX(m, 0, true), m.width - (m.pad + m.lw / 2));
  assert.ok(VT.laneX(m, 1, true) < VT.laneX(m, 0, true));
});

test('straight edge is a single line', () => {
  assert.equal(VT.edgePath(10, 20, 10, 10, 100, 18), 'M10,20 L10,100');
});

test('fork bends just above the parent', () => {
  const d = VT.edgePath(25, 20, 25, 10, 100, 18);
  assert.equal(d, 'M25,20 L25,82 C25,91 10,91 10,100');
});

test('merge edge bends right below the child', () => {
  const d = VT.edgePath(10, 20, 25, 25, 100, 18);
  assert.equal(d, 'M10,20 C10,29 25,29 25,38 L25,100');
});

test('double bend on a short span splits the height', () => {
  const d = VT.edgePath(10, 0, 20, 30, 20, 18);
  assert.equal(d, 'M10,0 C10,5 20,5 20,10 C20,15 30,15 30,20');
});

test('withDays inserts one separator per day and skips pending', () => {
  const t = new Date(2026, 8, 28, 12, 0, 0).getTime() / 1000;
  const rows = [
    { key: 'WORKTREE', pending: true, time: 9e15 },
    { key: 'a', time: t },
    { key: 'b', time: t - 3600 },
    { key: 'c', time: t - 86400 },
  ];
  const items = VT.withDays(rows);
  assert.deepEqual(
    items.map((i) => i.type + ':' + (i.row ? i.row.key : '')),
    ['row:WORKTREE', 'day:', 'row:a', 'row:b', 'day:', 'row:c'],
  );
  assert.equal(items[2].index, 1);
  assert.equal(new Set(items.map((i) => i.key)).size, items.length);
});

test('changeText summarises settings changes', () => {
  assert.equal(VT.changeText(null), '');
  assert.equal(VT.changeText({ initial: true, enabled: 1 }), 'First version · 1 service');
  assert.equal(VT.changeText({ unchanged: true }), 'No settings changes');
  assert.equal(
    VT.changeText({ added: ['a', 'b', 'c'], removed: ['d'], changed: ['e'], sections: ['core'] }),
    'Enabled a, b +1 · Disabled d · Changed e, core',
  );
  assert.equal(VT.changeText({ inputs: ['neo', 'nixpkgs'] }), 'Updated neo, nixpkgs');
});

test('quickTime keeps clock-only today and adds relative age otherwise', () => {
  // Fixed local wall clock: 2026-10-02 15:30
  const now = new Date(2026, 9, 2, 15, 30, 0).getTime() / 1000;
  const todayMorning = new Date(2026, 9, 2, 10, 35, 0).getTime() / 1000;
  const twoDaysAgo = new Date(2026, 8, 30, 10, 35, 0).getTime() / 1000;
  assert.equal(VT.clockTime(todayMorning), '10:35');
  assert.equal(VT.quickTime(todayMorning, now), '10:35');
  assert.equal(VT.sameCalendarDay(todayMorning, now), true);
  assert.equal(VT.sameCalendarDay(twoDaysAgo, now), false);
  assert.equal(VT.relTime(twoDaysAgo, now), '2 days ago');
  assert.equal(VT.quickTime(twoDaysAgo, now), '2 days ago · 10:35');
  assert.equal(VT.quickTime(0, now), '');
  assert.equal(VT.quickTime(null, now), '');
});

test('fullTime pads every field and is empty without a timestamp', () => {
  const ts = new Date(2026, 0, 2, 3, 4, 5).getTime() / 1000;
  assert.equal(VT.fullTime(ts), '2026-01-02 03:04:05');
  assert.equal(VT.fullTime(0), '');
});
