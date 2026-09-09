'use strict';
require('./registry.js');
require('./primary_item_list.js');
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { makeForm } = require('./test/harness.js');

function idsOption(current, uiExtra) {
  return {
    name: 'telegramAllowedUserId',
    type: { kind: 'listOf', elem: { kind: 'int' } },
    default: [],
    current: current || [],
    ui: Object.assign(
      {
        widget: 'primaryItemList',
        entryLabel: 'Home channel',
        emptyHint: 'Add at least one Telegram user id.',
      },
      uiExtra || {},
    ),
  };
}

function keyedOption(current) {
  return {
    name: 'perUser',
    type: {
      kind: 'attrsOf',
      elem: {
        kind: 'submodule',
        fields: [{ name: 'n', type: { kind: 'int' }, default: 0 }],
      },
    },
    default: {},
    current: current !== undefined ? current : {},
    ui: { keysFrom: { option: 'telegramAllowedUserId', extract: 'identity' } },
  };
}

test('pilEntryLabel uses ui.entryLabel and defaults to Primary', () => {
  const withLabel = makeForm({ options: [idsOption([1])] });
  assert.equal(withLabel.pilEntryLabel('telegramAllowedUserId'), 'Home channel');

  const bare = makeForm({
    options: [
      {
        name: 'telegramAllowedUserId',
        type: { kind: 'listOf', elem: { kind: 'int' } },
        default: [],
        current: [],
        ui: { widget: 'primaryItemList' },
      },
    ],
  });
  assert.equal(bare.pilEntryLabel('telegramAllowedUserId'), 'Primary');
});

test('pilEmptyHint uses ui.emptyHint and has a default string', () => {
  const withHint = makeForm({ options: [idsOption([1])] });
  assert.equal(
    withHint.pilEmptyHint('telegramAllowedUserId'),
    'Add at least one Telegram user id.',
  );

  const bare = makeForm({
    options: [
      {
        name: 'telegramAllowedUserId',
        type: { kind: 'listOf', elem: { kind: 'int' } },
        default: [],
        current: [],
        ui: { widget: 'primaryItemList' },
      },
    ],
  });
  assert.equal(
    bare.pilEmptyHint('telegramAllowedUserId'),
    'Add at least one entry. The first is the primary.',
  );
});

test('pilSetPrimary moves index to front', () => {
  const form = makeForm({ options: [idsOption([10, 20, 30])] });
  form.pilSetPrimary('telegramAllowedUserId', 2);
  assert.deepEqual(form.values.telegramAllowedUserId, [30, 10, 20]);

  const form2 = makeForm({ options: [idsOption([10, 20, 30])] });
  form2.pilSetPrimary('telegramAllowedUserId', 1);
  assert.deepEqual(form2.values.telegramAllowedUserId, [20, 10, 30]);
});

test('pilSetPrimary no-ops for idx 0, negative, and out of range', () => {
  const form = makeForm({ options: [idsOption([10, 20, 30])] });
  const orig = [10, 20, 30];

  form.pilSetPrimary('telegramAllowedUserId', 0);
  assert.deepEqual(form.values.telegramAllowedUserId, orig);

  form.pilSetPrimary('telegramAllowedUserId', -1);
  assert.deepEqual(form.values.telegramAllowedUserId, orig);

  form.pilSetPrimary('telegramAllowedUserId', 3);
  assert.deepEqual(form.values.telegramAllowedUserId, orig);

  form.pilSetPrimary('telegramAllowedUserId', 99);
  assert.deepEqual(form.values.telegramAllowedUserId, orig);
});

test('pilSetPrimary notifyKeysFromSource reorders attrsOf keysFrom consumer', () => {
  const form = makeForm({
    options: [
      idsOption([10, 20, 30]),
      keyedOption({ 10: { n: 1 }, 30: { n: 9 } }),
    ],
  });
  assert.equal(form.values.perUser['10'].n, 1);
  assert.equal(form.values.perUser['30'].n, 9);

  form.pilSetPrimary('telegramAllowedUserId', 2);
  assert.deepEqual(form.values.telegramAllowedUserId, [30, 10, 20]);
  // deriveKeysFrom follows list order; integer-like Object.keys are sorted by engine.
  assert.deepEqual(
    form.deriveKeysFrom(form.optionsByName.perUser.ui.keysFrom),
    ['30', '10', '20'],
  );
  assert.deepEqual(
    Object.keys(form.values.perUser).sort(),
    ['10', '20', '30'],
  );
  assert.equal(form.values.perUser['10'].n, 1);
  assert.equal(form.values.perUser['30'].n, 9);
});

test('collectSave uses generic isAtDefault — no prepareSave', () => {
  const widget = globalThis.NeoWidgets.get('primaryItemList');
  assert.ok(widget);
  assert.equal(typeof widget.prepareSave, 'undefined');
  assert.equal(typeof widget.isAtDefault, 'undefined');
  assert.equal(typeof widget.init, 'undefined');

  const form = makeForm({ options: [idsOption([])] });
  assert.deepEqual(form.collectSave(), {});

  form.values.telegramAllowedUserId = [42];
  assert.deepEqual(form.collectSave(), { telegramAllowedUserId: [42] });

  form.addListItem('telegramAllowedUserId');
  // addListItem pushes default int 0 → [42, 0]
  let saved = form.collectSave();
  assert.ok('telegramAllowedUserId' in saved);
  assert.deepEqual(saved.telegramAllowedUserId, [42, 0]);

  form.values.telegramAllowedUserId = [];
  assert.deepEqual(form.collectSave(), {});

  // From empty default, add one item then revert via setting default
  const form2 = makeForm({ options: [idsOption([])] });
  form2.addListItem('telegramAllowedUserId');
  form2.values.telegramAllowedUserId = [42];
  assert.deepEqual(form2.collectSave(), { telegramAllowedUserId: [42] });
  form2.values.telegramAllowedUserId = [];
  assert.ok(form2.isAtDefault('telegramAllowedUserId'));
  assert.deepEqual(form2.collectSave(), {});
});

test('pilSetPrimary ensureList creates missing list; empty list no-ops', () => {
  const form = makeForm({
    options: [
      {
        name: 'telegramAllowedUserId',
        type: { kind: 'listOf', elem: { kind: 'int' } },
        default: [],
        current: [],
        ui: { widget: 'primaryItemList', entryLabel: 'Home channel' },
      },
    ],
  });
  delete form.values.telegramAllowedUserId;
  form.pilSetPrimary('telegramAllowedUserId', 1);
  assert.ok(Array.isArray(form.values.telegramAllowedUserId));
  assert.deepEqual(form.values.telegramAllowedUserId, []);

  form.pilSetPrimary('telegramAllowedUserId', 0);
  assert.deepEqual(form.values.telegramAllowedUserId, []);
});
