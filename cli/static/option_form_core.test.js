const test = require('node:test');
const assert = require('node:assert/strict');
const core = require('./option_form_core.js');

test('cloneValue copies deeply and keeps null/undefined', () => {
  const v = { a: [1, { b: 2 }] };
  const c = core.cloneValue(v);
  assert.deepEqual(c, v);
  assert.notEqual(c.a, v.a);
  assert.equal(core.cloneValue(null), null);
  assert.equal(core.cloneValue(undefined), undefined);
});

test('deepEqual compares by JSON', () => {
  assert.equal(core.deepEqual({ a: [1] }, { a: [1] }), true);
  assert.equal(core.deepEqual({ a: 1 }, { a: 2 }), false);
  const loop = {};
  loop.self = loop;
  assert.equal(core.deepEqual(loop, loop), false);
});

test('defaultForType gives the empty value of each kind', () => {
  assert.equal(core.defaultForType({ kind: 'bool' }), false);
  assert.equal(core.defaultForType({ kind: 'port', min: 1 }), 1);
  assert.equal(core.defaultForType({ kind: 'int' }), 0);
  assert.equal(core.defaultForType({ kind: 'separatedString' }), '');
  assert.equal(core.defaultForType({ kind: 'enum', values: ['a', 'b'] }), 'a');
  assert.equal(core.defaultForType({ kind: 'enum' }), '');
  assert.deepEqual(core.defaultForType({ kind: 'listOf' }), []);
  assert.deepEqual(core.defaultForType({ kind: 'attrsOf' }), {});
  assert.equal(core.defaultForType({ kind: 'nullOr', elem: { kind: 'bool' } }), false);
  assert.equal(core.defaultForType(null), null);
  assert.equal(core.defaultForType({ kind: 'weird' }), null);
});

test('defaultForType fills submodule fields: default, null for nullOr, else the type', () => {
  const t = {
    kind: 'submodule',
    fields: [
      { name: 'port', type: { kind: 'port' }, default: 80 },
      { name: 'note', type: { kind: 'nullOr', elem: { kind: 'str' } } },
      { name: 'on', type: { kind: 'bool' } },
      { name: 'tags', type: { kind: 'listOf' }, default: ['x'] },
    ],
  };
  const v = core.defaultForType(t);
  assert.deepEqual(v, { port: 80, note: null, on: false, tags: ['x'] });
  v.tags.push('y');
  assert.deepEqual(t.fields[3].default, ['x']);
});

test('mergeSubmoduleValue keeps set fields and fills missing ones', () => {
  const t = { kind: 'submodule', fields: [{ name: 'a', type: { kind: 'str' } }, { name: 'b', type: { kind: 'bool' } }] };
  assert.deepEqual(core.mergeSubmoduleValue({ a: 'x' }, t), { a: 'x', b: false });
  assert.deepEqual(core.mergeSubmoduleValue(['bad'], t), { a: '', b: false });
});

test('normalizeValue: lists, attrs, submodules and scalars', () => {
  const sub = { kind: 'submodule', fields: [{ name: 'n', type: { kind: 'int' } }] };
  assert.deepEqual(core.normalizeValue({ type: { kind: 'listOf' }, default: ['d'] }, null), ['d']);
  assert.deepEqual(core.normalizeValue({ type: { kind: 'listOf', elem: sub } }, [{}]), [{ n: 0 }]);
  assert.deepEqual(core.normalizeValue({ type: { kind: 'attrsOf' }, default: { k: 1 } }, 'x'), { k: 1 });
  assert.deepEqual(core.normalizeValue({ type: { kind: 'attrsOf' }, default: [] }, undefined), {});
  assert.deepEqual(core.normalizeValue({ type: { kind: 'attrsOf', elem: sub } }, { a: {} }), { a: { n: 0 } });
  assert.deepEqual(core.normalizeValue({ type: sub }, null), { n: 0 });
  assert.equal(core.normalizeValue({ type: { kind: 'str' } }, undefined), null);
  assert.equal(core.normalizeValue({}, 'x'), 'x');
});

test('keysFrom: deriveKeys and syncKeys', () => {
  assert.deepEqual(core.deriveKeys({ option: 'users', extract: 'beforeColon' }, ['alice:hash', 'bob', ':x']), ['alice', 'bob']);
  assert.deepEqual(core.deriveKeys({ option: 'users' }, ['a:b']), ['a:b']);
  assert.deepEqual(core.deriveKeys({ option: 'users' }, { x: 1, y: 2 }), ['x', 'y']);
  assert.deepEqual(core.deriveKeys({ option: 'users' }, 'str'), []);
  assert.deepEqual(core.deriveKeys(null, ['a']), []);
  const elem = { kind: 'submodule', fields: [{ name: 'allow', type: { kind: 'listOf' } }] };
  const prev = { alice: { allow: ['x'] }, gone: { allow: [] } };
  assert.deepEqual(core.syncKeys(['bob', 'alice'], prev, elem), { bob: { allow: [] }, alice: { allow: ['x'] } });
  assert.deepEqual(Object.keys(core.syncKeys(['bob', 'alice'], prev, elem)), ['bob', 'alice']);
  assert.deepEqual(core.syncKeys(['a'], null, null), { a: {} });
});

test('patternErrorForType mirrors whole-string builtins.match', () => {
  const t = { kind: 'strMatching', pattern: '[a-z]+' };
  assert.equal(core.patternErrorForType(t, 'abc'), '');
  assert.equal(core.patternErrorForType(t, 'abc1'), 'Must match /[a-z]+/');
  assert.equal(core.patternErrorForType({ kind: 'nullOr', elem: t }, ''), '');
  assert.equal(core.patternErrorForType({ kind: 'nullOr', elem: t }, 'A'), 'Must match /[a-z]+/');
  assert.equal(core.patternErrorForType({ kind: 'strMatching', pattern: '(' }, 'x'), '');
  assert.equal(core.patternErrorForType({ kind: 'str' }, 'x'), '');
  assert.equal(core.patternErrorForType(null, 'x'), '');
});

test('separatedString lines round-trip through the separator', () => {
  assert.equal(core.sepToLines('a,b', ','), 'a\nb');
  assert.equal(core.sepToLines(null, ','), '');
  assert.equal(core.sepToLines('a\nb', undefined), 'a\nb');
  assert.equal(core.linesToSep('a\nb', ','), 'a,b');
  assert.equal(core.linesToSep('a\nb', ''), 'a\nb');
});

test('invalidSummary names the first problem and counts the rest', () => {
  assert.equal(core.invalidSummary(['A']), 'A');
  assert.equal(core.invalidSummary(['A', 'B', 'C']), 'A (+2 more)');
});

test('visibleWhen rules for options and entry fields', () => {
  assert.deepEqual(core.splitVisibleWhen('a=b,c'), ['a', 'b,c']);
  assert.deepEqual(core.splitVisibleWhen(''), [null, undefined]);
  const values = { on: false, type: 'entra', n: 0 };
  assert.equal(core.fieldVisible(undefined, values), true);
  assert.equal(core.fieldVisible('on', values), false);
  assert.equal(core.fieldVisible('missing', values), true);
  assert.equal(core.fieldVisible('type=entra', values), true);
  assert.equal(core.fieldVisible('type=google,github', values), false);
  assert.equal(core.fieldVisible('n=0', values), true);
  assert.equal(core.entryFieldVisible('svc.destinations.format=files,encrypted', { format: 'zfs' }), false);
  assert.equal(core.entryFieldVisible('svc.destinations.format=files,encrypted', { format: 'files' }), true);
  assert.equal(core.entryFieldVisible('enabled', { other: 1 }), true);
  assert.equal(core.entryFieldVisible('enabled', undefined), true);
  assert.equal(core.entryFieldVisible('', { enabled: false }), true);
});

test('choices: list from the type or its element, labels by value', () => {
  const t = { kind: 'enum', choices: [{ value: 'web', label: 'Internet' }, { value: 1, label: 'One' }] };
  assert.equal(core.choicesOf(t).length, 2);
  assert.equal(core.choicesOf({ elem: t }).length, 2);
  assert.deepEqual(core.choicesOf(null), []);
  assert.equal(core.choiceLabel(t, 'web'), 'Internet');
  assert.equal(core.choiceLabel(t, '1'), 'One');
  assert.equal(core.choiceLabel(t, 'x'), 'x');
});

test('toggleInList adds once and removes, on a copy', () => {
  const list = ['a'];
  assert.deepEqual(core.toggleInList(list, 'b', true), ['a', 'b']);
  assert.deepEqual(core.toggleInList(list, 'a', true), ['a']);
  assert.deepEqual(core.toggleInList(list, 'a', false), []);
  assert.deepEqual(list, ['a']);
  assert.deepEqual(core.toggleInList(null, 'a', true), ['a']);
});

test('summaryChip per value shape', () => {
  const up = (x) => String(x).toUpperCase();
  assert.deepEqual(core.summaryChip('SSO', true, up), { text: '✓ SSO', cls: 'badge-soft badge-primary' });
  assert.equal(core.summaryChip('SSO', false, up).text, '✕ SSO');
  assert.equal(core.summaryChip('Ingress', [], up).text, 'Ingress: none');
  assert.equal(core.summaryChip('Ingress', ['a', 'b', 'c', 'd', 'e'], up).text, 'Ingress: A, B, C +2');
  assert.equal(core.summaryChip('Name', '', up).text, 'Name: not set');
  assert.equal(core.summaryChip('Users', { a: 1, b: 2 }, up).text, 'Users: 2');
  assert.equal(core.summaryChip('Mode', 'x', up).text, 'Mode: X');
});

test('pruneEntryDefaults drops fields still at their entry default', () => {
  const ed = { app: { schedule: 'daily', keep: 3 } };
  assert.deepEqual(
    core.pruneEntryDefaults(ed, { app: { schedule: 'daily', keep: 5 }, other: { x: 1 } }),
    { app: { keep: 5 }, other: { x: 1 } },
  );
  assert.deepEqual(core.pruneEntryDefaults(ed, { app: { schedule: 'daily' } }), { app: {} });
  assert.equal(core.pruneEntryDefaults(null, 'v'), 'v');
  assert.deepEqual(core.pruneEntryDefaults(ed, ['x']), ['x']);
});

test('nested submodule fields of an entry load flat and save nested', () => {
  // attrsOf submodule whose entry holds a plain submodule `postgres`:
  // the fields arrive dotted, settings.toml holds them nested.
  const elem = {
    kind: 'submodule',
    fields: [
      { name: 'label', type: { kind: 'str' }, default: '' },
      { name: 'postgres.user', type: { kind: 'str' }, default: 'kpi' },
      { name: 'postgres.password', type: { kind: 'nullOr', elem: { kind: 'str' } }, secret: true },
    ],
  };
  const opt = { type: { kind: 'attrsOf', elem } };
  const saved = { bank: { label: 'Bank', postgres: { password: '@NEO_SECRET:x' } } };

  const form = core.normalizeValue(opt, saved);
  assert.deepEqual(form.bank, {
    label: 'Bank',
    'postgres.user': 'kpi',
    'postgres.password': '@NEO_SECRET:x',
  });

  form.bank['postgres.password'] = 'typed';
  const out = core.nestValue(opt.type, form);
  assert.deepEqual(out, { bank: { label: 'Bank', postgres: { user: 'kpi', password: 'typed' } } });
  assert.equal(JSON.stringify(out).includes('postgres.'), false);

  // A cleared field (null) is left out; other nested keys stay.
  const cleared = core.nestEntry({ 'postgres.password': null, postgres: { extra: 1 } }, elem);
  assert.deepEqual(cleared, { postgres: { extra: 1 } });

  // listOf entries and dotted map keys inside a field value are handled / kept.
  const list = { kind: 'listOf', elem };
  assert.deepEqual(core.nestValue(list, [{ 'postgres.user': 'a' }]), [{ postgres: { user: 'a' } }]);
  const withMap = {
    kind: 'submodule',
    fields: [{ name: 'tokens', type: { kind: 'attrsOf', elem: { kind: 'str' } } }],
  };
  assert.deepEqual(core.nestEntry({ tokens: { 'github.com': 't' } }, withMap), { tokens: { 'github.com': 't' } });
  // Not a submodule collection: unchanged.
  assert.deepEqual(core.nestValue({ kind: 'attrsOf', elem: { kind: 'str' } }, { 'a.b': 'c' }), { 'a.b': 'c' });
});
