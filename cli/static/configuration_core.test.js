'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const C = require('./configuration_core.js');

/** A stand-in for #config-content holding the given ids (with attributes). */
function content(nodes) {
  return {
    querySelector(sel) {
      const n = nodes[sel.replace(/^#/, '')];
      return n ? { getAttribute: (k) => (n[k] === undefined ? null : n[k]) } : null;
    },
  };
}

test('tab labels and URLs', () => {
  assert.equal(C.sectionLabel('settings'), 'Settings');
  assert.equal(C.sectionLabel('versioning'), 'Versioning');
  assert.equal(C.sectionLabel('guides'), 'Guides');
  assert.equal(C.sectionLabel('services'), 'Services');
  assert.equal(C.sectionLabel(undefined), 'Services');
  assert.equal(C.tabUrl('settings'), '/configuration/settings');
  assert.equal(C.tabUrl('versioning'), '/configuration/versioning');
  assert.equal(C.tabUrl('guides'), '/configuration/guides');
  assert.equal(C.tabUrl('services'), '/configuration');
});

test('isNavRequest recognises #config-content however htmx reports it', () => {
  const elt = (attrs) => ({ getAttribute: (k) => attrs[k] ?? null });
  assert.equal(C.isConfigContentTarget('#config-content'), true);
  assert.equal(C.isConfigContentTarget('config-content'), true);
  assert.equal(C.isConfigContentTarget('#changes-body'), false);
  assert.equal(C.isConfigContentTarget({ id: 'config-content' }), true);
  assert.equal(C.isConfigContentTarget(elt({ 'hx-target': '#config-content' })), true);
  assert.equal(C.isConfigContentTarget({ id: 'x' }), false);
  assert.equal(C.isConfigContentTarget(null), false);
  assert.equal(C.isNavRequest({ elt: elt({ 'hx-target': '#config-content' }) }), true);
  assert.equal(C.isNavRequest({ requestConfig: { target: '#config-content' } }), true);
  assert.equal(C.isNavRequest({ target: { id: 'action-bar-dynamic' } }), false);
});

test('formDirty asks the form for every value', () => {
  const form = (dirty) => ({ values: { a: 1, b: 2 }, isAtOriginal: (k) => !dirty.includes(k) });
  assert.equal(C.formDirty(form([])), false);
  assert.equal(C.formDirty(form(['b'])), true);
  assert.equal(C.formDirty(null), false);
  assert.equal(C.formDirty({ values: { a: 1 } }), false);
});

test('discardsChanges for reset and revert only', () => {
  assert.equal(C.discardsChanges('/actions/reset'), true);
  assert.equal(C.discardsChanges('/changes/revert'), true);
  assert.equal(C.discardsChanges('/actions/activate'), false);
});

test('shellState: option pane, guide, privacy map, grids', () => {
  const pane = C.shellState(content({ 'options-pane': { 'data-service': 'immich', 'data-is-core': 'false' } }));
  assert.equal(pane.detail, 'immich');
  assert.equal(pane.tab, 'services');
  assert.ok(pane.pane);
  assert.equal(C.shellState(content({ 'options-pane': { 'data-service': 'core', 'data-is-core': 'true' } })).tab, 'settings');
  assert.deepEqual(C.shellState(content({ 'guide-page': { 'data-guide-title': 'Block ads' } })), {
    pane: null, detail: 'Block ads', tab: 'guides',
  });
  assert.equal(C.shellState(content({ 'guide-page': {} })).detail, 'Guide');
  assert.deepEqual(C.shellState(content({ 'privacy-map': {} })), { pane: null, detail: 'Privacy map', tab: 'guides' });
  assert.equal(C.shellState(content({ 'core-grid': {} })).tab, 'settings');
  assert.equal(C.shellState(content({ 'branches-section': {} })).tab, 'versioning');
  assert.equal(C.shellState(content({ 'versioning-root': {} })).tab, 'versioning');
  assert.equal(C.shellState(content({ 'guides-grid': {} })).tab, 'guides');
  assert.deepEqual(C.shellState(content({ 'services-grid': {} })), { pane: null, detail: null, tab: 'services' });
  assert.deepEqual(C.shellState(content({})), { pane: null, detail: null, tab: null });
  assert.deepEqual(C.shellState(null), { pane: null, detail: null, tab: null });
});

test('fusedPlate only while the save strip is stuck', () => {
  const action = { top: 60, left: 16, width: 800 };
  assert.equal(C.fusedPlate(120, action, { top: 300, bottom: 350 }), null);
  assert.deepEqual(C.fusedPlate(120, action, { top: 120.4, bottom: 170 }), { top: 60, left: 16, width: 800, height: 110 });
  assert.equal(C.fusedPlate(120, action, { top: 20, bottom: 40 }).height, 0);
});

test('isStaleUnitSwap matches unit-controls and update-out OOB rows', () => {
  assert.equal(C.isStaleUnitSwap('<div id="unit-controls-docker-x">'), true);
  assert.equal(C.isStaleUnitSwap('<div id="update-out-immich">'), true);
  assert.equal(C.isStaleUnitSwap('<div id="other">'), false);
  assert.equal(C.isStaleUnitSwap(undefined), false);
});
