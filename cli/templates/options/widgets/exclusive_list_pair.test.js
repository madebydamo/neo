'use strict';

require('./registry.js');
require('./exclusive_list_pair.js');
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { makeForm } = require('./test/harness.js');

function accessOption(overrides) {
  return {
    name: 'access',
    type: {
      kind: 'attrsOf',
      elem: {
        kind: 'submodule',
        fields: [
          {
            name: 'allow',
            type: {
              kind: 'listOf',
              elem: { kind: 'str' },
              values: ['immich', 'paperless', 'vaultwarden'],
            },
          },
          {
            name: 'block',
            type: {
              kind: 'listOf',
              elem: { kind: 'str' },
              values: ['immich', 'paperless', 'vaultwarden'],
            },
          },
        ],
      },
    },
    default: {},
    ui: {
      widget: 'exclusiveListPair',
      keysFrom: { option: 'users', extract: 'beforeColon' },
      modes: [
        {
          id: 'open',
          label: 'All apps',
          active: [],
          badge: 'success',
          hintEmpty: 'No restrictions',
          hintFilled: 'No restrictions',
        },
        {
          id: 'allow',
          label: 'Allow list',
          active: ['allow'],
          listLabel: 'Allowed apps',
          badge: 'primary',
          hintEmpty: 'Pick at least one',
          hintFilled: 'only checked',
        },
        {
          id: 'block',
          label: 'Block list',
          active: ['block'],
          listLabel: 'Blocked apps',
          badge: 'warning',
          hintEmpty: 'Pick apps to block',
          hintFilled: 'except checked',
        },
      ],
      save: { pruneEmptyEntries: true, omitIfEmpty: true },
      entryLabel: 'User',
      emptyHint: 'Add users above',
      choiceEmptyHint: 'No enabled apps',
    },
    ...overrides,
  };
}

function usersOption(current) {
  return {
    name: 'users',
    type: { kind: 'listOf', elem: { kind: 'str' } },
    default: [],
    current,
  };
}

function makeAccessForm(opts) {
  opts = opts || {};
  const users = opts.users !== undefined ? opts.users : ['alice:HASH', 'bob:HASH'];
  const access = opts.access !== undefined ? opts.access : {};
  const accessOpt = accessOption(opts.accessOptionOverrides || {});
  if (opts.accessCurrent !== undefined) accessOpt.current = opts.accessCurrent;
  else if (Object.keys(access).length) accessOpt.current = access;

  return makeForm({
    options: [usersOption(users), accessOpt],
    values: {
      users,
      access,
      ...(opts.values || {}),
    },
    defaults: opts.defaults,
    originals: opts.originals,
    $watch: opts.$watch || null,
  });
}

test('registers exclusiveListPair on NeoWidgets', () => {
  const w = globalThis.NeoWidgets.get('exclusiveListPair');
  assert.ok(w);
  assert.equal(w.name, 'exclusiveListPair');
  assert.equal(typeof w.init, 'function');
  assert.equal(typeof w.prepareSave, 'function');
  assert.equal(typeof w.isAtDefault, 'function');
  assert.equal(typeof w.mixins.elpMode, 'function');
  assert.equal(typeof w.mixins.setElpMode, 'function');
  assert.equal(typeof w.mixins.toggleElpApp, 'function');
});

test('keysFrom sync: seeds keys from users, drops deleted, adds new', () => {
  const form = makeAccessForm({
    users: ['alice:HASH', 'bob:HASH'],
    access: {},
  });
  form.initWidgets();

  assert.deepEqual(Object.keys(form.values.access).sort(), ['alice', 'bob']);
  assert.deepEqual(form.values.access.alice, { allow: [], block: [] });
  assert.deepEqual(form.values.access.bob, { allow: [], block: [] });

  form.values.users = ['alice:HASH'];
  form.syncKeysFromOption('access');
  assert.deepEqual(Object.keys(form.values.access), ['alice']);
  assert.ok(!('bob' in form.values.access));

  form.values.users = ['alice:HASH', 'carol:HASH'];
  form.notifyKeysFromSource('users');
  assert.deepEqual(Object.keys(form.values.access).sort(), ['alice', 'carol']);
  assert.deepEqual(form.values.access.carol, { allow: [], block: [] });
});

test('mode inference from data: allow / block / open + labels and badges', () => {
  const form = makeAccessForm({
    users: ['alice:HASH', 'bob:HASH', 'carol:HASH'],
    access: {
      alice: { allow: ['immich'], block: [] },
      bob: { allow: [], block: ['paperless'] },
      carol: { allow: [], block: [] },
    },
  });
  form.initWidgets();

  assert.equal(form.elpMode('access', 'alice'), 'allow');
  assert.equal(form.elpModeLabel('access', 'alice'), 'Allow list');
  assert.equal(form.elpBadgeClass('access', 'alice'), 'badge-primary badge-outline');
  assert.equal(form.elpListLabel('access', 'alice'), 'Allowed apps');

  assert.equal(form.elpMode('access', 'bob'), 'block');
  assert.equal(form.elpModeLabel('access', 'bob'), 'Block list');
  assert.equal(form.elpBadgeClass('access', 'bob'), 'badge-warning badge-outline');
  assert.equal(form.elpListLabel('access', 'bob'), 'Blocked apps');

  assert.equal(form.elpMode('access', 'carol'), 'open');
  assert.equal(form.elpModeLabel('access', 'carol'), 'All apps');
  assert.equal(form.elpBadgeClass('access', 'carol'), 'badge-success badge-outline');
});

test('sticky UI mode: empty lists keep setElpMode; sticky survives clear', () => {
  const form = makeAccessForm({
    users: ['alice:HASH'],
    access: { alice: { allow: [], block: [] } },
  });
  form.initWidgets();
  assert.equal(form.elpMode('access', 'alice'), 'open');

  form.setElpMode('access', 'alice', 'allow');
  assert.equal(form.elpMode('access', 'alice'), 'allow');
  assert.deepEqual(form.values.access.alice.allow, []);
  assert.deepEqual(form.values.access.alice.block, []);

  form.toggleElpApp('access', 'alice', 'immich', true);
  assert.deepEqual(form.values.access.alice.allow, ['immich']);
  form.toggleElpApp('access', 'alice', 'immich', false);
  assert.deepEqual(form.values.access.alice.allow, []);
  assert.equal(form.elpMode('access', 'alice'), 'allow');
});

test('data wins over sticky: non-empty allow overrides uiState block', () => {
  const form = makeAccessForm({
    users: ['alice:HASH'],
    access: { alice: { allow: ['immich'], block: [] } },
  });
  form.initWidgets();
  form.elpEnsureState('access');
  form.uiState.access.modes.alice = 'block';
  form.elpSyncModes('access');
  assert.equal(form.elpMode('access', 'alice'), 'allow');
});

test('setElpMode carry-over between allow / block / open', () => {
  const form = makeAccessForm({
    users: ['alice:HASH'],
    access: { alice: { allow: ['immich'], block: [] } },
  });
  form.initWidgets();

  form.setElpMode('access', 'alice', 'block');
  assert.deepEqual(form.values.access.alice.block, ['immich']);
  assert.deepEqual(form.values.access.alice.allow, []);
  assert.equal(form.elpMode('access', 'alice'), 'block');

  form.setElpMode('access', 'alice', 'open');
  assert.deepEqual(form.values.access.alice.allow, []);
  assert.deepEqual(form.values.access.alice.block, []);
  assert.equal(form.elpMode('access', 'alice'), 'open');

  form.setElpMode('access', 'alice', 'allow');
  assert.deepEqual(form.values.access.alice.allow, []);
  assert.deepEqual(form.values.access.alice.block, []);
});

test('toggleElpApp writes active list; open mode is a no-op', () => {
  const form = makeAccessForm({
    users: ['alice:HASH'],
    access: { alice: { allow: [], block: [] } },
  });
  form.initWidgets();

  form.setElpMode('access', 'alice', 'allow');
  form.toggleElpApp('access', 'alice', 'immich', true);
  assert.deepEqual(form.values.access.alice.allow, ['immich']);
  assert.deepEqual(form.values.access.alice.block, []);
  assert.equal(form.elpAppSelected('access', 'alice', 'immich'), true);

  form.toggleElpApp('access', 'alice', 'immich', false);
  assert.deepEqual(form.values.access.alice.allow, []);
  assert.equal(form.elpAppSelected('access', 'alice', 'immich'), false);

  form.setElpMode('access', 'alice', 'open');
  form.toggleElpApp('access', 'alice', 'immich', true);
  assert.deepEqual(form.values.access.alice.allow, []);
  assert.deepEqual(form.values.access.alice.block, []);
});

test('setAllElpApps select all / none on active list', () => {
  const form = makeAccessForm({
    users: ['alice:HASH'],
    access: { alice: { allow: [], block: [] } },
  });
  form.initWidgets();
  form.setElpMode('access', 'alice', 'allow');

  form.setAllElpApps('access', 'alice', true);
  assert.deepEqual(form.values.access.alice.allow, ['immich', 'paperless', 'vaultwarden']);
  assert.deepEqual(form.values.access.alice.block, []);

  form.setAllElpApps('access', 'alice', false);
  assert.deepEqual(form.values.access.alice.allow, []);
});

test('elpChoices reads type.values; empty array when missing', () => {
  const form = makeAccessForm();
  form.initWidgets();
  assert.deepEqual(form.elpChoices('access'), ['immich', 'paperless', 'vaultwarden']);

  const bare = makeForm({
    options: [
      usersOption(['alice:HASH']),
      accessOption({
        type: {
          kind: 'attrsOf',
          elem: {
            kind: 'submodule',
            fields: [
              { name: 'allow', type: { kind: 'listOf', elem: { kind: 'str' } } },
              { name: 'block', type: { kind: 'listOf', elem: { kind: 'str' } } },
            ],
          },
        },
      }),
    ],
    values: { users: ['alice:HASH'], access: {} },
  });
  bare.initWidgets();
  assert.deepEqual(bare.elpChoices('access'), []);
  assert.notEqual(bare.elpChoices('access'), undefined);
});

test('prepareSave prune+omit: empty map omitted; non-empty user kept', () => {
  const form = makeAccessForm({
    users: ['alice:HASH', 'bob:HASH'],
    access: {
      alice: { allow: [], block: [] },
      bob: { allow: [], block: [] },
    },
  });
  form.initWidgets();
  const emptySave = form.collectSave();
  assert.ok(!('access' in emptySave));

  form.setElpMode('access', 'alice', 'allow');
  form.toggleElpApp('access', 'alice', 'immich', true);
  const save = form.collectSave();
  assert.deepEqual(save.access, { alice: { allow: ['immich'], block: [] } });
  assert.ok(!('bob' in save.access));
});

test('isAtDefault: pruned empty map is at default even with empty entry shells', () => {
  const form = makeAccessForm({
    users: ['alice:HASH'],
    access: { alice: { allow: [], block: [] } },
  });
  form.initWidgets();
  assert.equal(form.elpIsAtDefault('access'), true);
  assert.equal(form.isAtDefault('access'), true);

  form.setElpMode('access', 'alice', 'allow');
  form.toggleElpApp('access', 'alice', 'immich', true);
  assert.equal(form.elpIsAtDefault('access'), false);
});

test('hints: filled vs empty; label fallbacks when ui omits them', () => {
  const form = makeAccessForm({
    users: ['alice:HASH'],
    access: { alice: { allow: [], block: [] } },
  });
  form.initWidgets();
  form.setElpMode('access', 'alice', 'allow');
  assert.equal(form.elpModeHint('access', 'alice'), 'Pick at least one');
  form.toggleElpApp('access', 'alice', 'immich', true);
  assert.equal(form.elpModeHint('access', 'alice'), 'only checked');

  assert.equal(form.elpEmptyHint('access'), 'Add users above');
  assert.equal(form.elpEntryLabel('access'), 'User');
  assert.equal(form.elpChoiceEmptyHint('access'), 'No enabled apps');

  const bare = makeForm({
    options: [
      usersOption(['alice:HASH']),
      accessOption({
        ui: {
          widget: 'exclusiveListPair',
          keysFrom: { option: 'users', extract: 'beforeColon' },
          modes: [
            { id: 'open', label: 'All apps', active: [], badge: 'success' },
            {
              id: 'allow',
              label: 'Allow list',
              active: ['allow'],
              badge: 'primary',
              hintEmpty: 'empty-allow',
            },
          ],
          save: { pruneEmptyEntries: true, omitIfEmpty: true },
        },
      }),
    ],
    values: { users: ['alice:HASH'], access: { alice: { allow: [], block: [] } } },
  });
  bare.initWidgets();
  assert.equal(bare.elpEmptyHint('access'), '');
  assert.equal(bare.elpEntryLabel('access'), 'Entry');
  assert.equal(bare.elpChoiceEmptyHint('access'), 'No choices available.');
});

test('resetField re-syncs keysFrom; revertField restores originals and modes', () => {
  const form = makeAccessForm({
    users: ['alice:HASH', 'bob:HASH'],
    access: {
      alice: { allow: ['immich'], block: [] },
      bob: { allow: [], block: [] },
    },
  });
  form.initWidgets();
  const origAccess = JSON.parse(JSON.stringify(form.values.access));
  const origMode = form.elpMode('access', 'alice');
  assert.equal(origMode, 'allow');

  form.setElpMode('access', 'alice', 'block');
  // Mode switch carries immich into block; clear then pick paperless for a dirty state.
  form.setAllElpApps('access', 'alice', false);
  form.toggleElpApp('access', 'alice', 'paperless', true);
  assert.equal(form.elpMode('access', 'alice'), 'block');
  assert.deepEqual(form.values.access.alice.block, ['paperless']);
  assert.deepEqual(form.values.access.alice.allow, []);

  form.revertField('access');
  assert.deepEqual(form.values.access, origAccess);
  assert.equal(form.elpMode('access', 'alice'), 'allow');

  form.values.access = {};
  form.resetField('access');
  assert.deepEqual(Object.keys(form.values.access).sort(), ['alice', 'bob']);
  assert.deepEqual(form.values.access.alice, { allow: [], block: [] });
  assert.deepEqual(form.values.access.bob, { allow: [], block: [] });
});
