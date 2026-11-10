'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const S = require('./eval_sync.js');

// Minimal element stand-in: attributes, id, contains, listeners.
function el(attrs, id) {
  const a = Object.assign({}, attrs);
  const listeners = {};
  return {
    id: id || '',
    tagName: 'DIV',
    getAttribute: (k) => (k in a ? a[k] : null),
    setAttribute: (k, v) => {
      a[k] = String(v);
    },
    contains: () => false,
    querySelector: () => null,
    addEventListener: (name, fn) => {
      listeners[name] = fn;
    },
    attrs: a,
    listeners,
  };
}

function frag(keys, gen, provisional, url, id) {
  return el(
    {
      'data-neo-eval-keys': keys,
      'data-neo-eval-gen': String(gen),
      'data-neo-eval-provisional': provisional ? 'true' : 'false',
      'data-neo-eval-url': url,
    },
    id
  );
}

function doc(nodes, opts) {
  const body = el({ 'data-theme': 'light' });
  return {
    body,
    activeElement: (opts && opts.active) || body,
    querySelectorAll: () => nodes,
    getElementById: () => null,
  };
}

function sync(gen, changed, overlaid, extra) {
  return Object.assign(
    { gen, pending: false, theme: '', changed: changed || {}, changedOverlaid: overlaid || {} },
    extra
  );
}

function deps(dirty) {
  const calls = [];
  return {
    calls,
    refresh: (node, url) => calls.push(url),
    dirty: () => !!dirty,
  };
}

test('readSync parses the pushed element', () => {
  const s = S.readSync(
    el({
      'data-gen': '7',
      'data-pending': 'true',
      'data-theme': 'dark',
      'data-changed': '{"services":7}',
      'data-changed-overlaid': 'not json',
    })
  );
  assert.deepEqual(s, {
    gen: 7,
    pending: true,
    theme: 'dark',
    changed: { services: 7 },
    changedOverlaid: {},
  });
  assert.equal(S.readSync(null), null);
});

test('needsRefresh: raw changes for plain fragments, overlay changes for provisional ones', () => {
  const s = sync(5, { 'pane:service:a': 5, guides: 3 }, { guides: 3 });
  const plain = { keys: ['pane:service:a', 'guides'], gen: 4, provisional: false };
  const prov = { keys: ['pane:service:a', 'guides'], gen: 4, provisional: true };
  assert.equal(S.needsRefresh(plain, s), true);
  // The overlay already showed the saved value: nothing new for this pane.
  assert.equal(S.needsRefresh(prov, s), false);
  // A change older than the fragment does not count.
  assert.equal(S.needsRefresh({ keys: ['guides'], gen: 4, provisional: false }, s), false);
  // Rendered from a generation missed by the client: still caught.
  assert.equal(S.needsRefresh({ keys: ['guides'], gen: 2, provisional: true }, s), true);
  // Same or newer generation, or unknown generation: nothing.
  assert.equal(S.needsRefresh({ keys: ['pane:service:a'], gen: 5, provisional: false }, s), false);
  assert.equal(S.needsRefresh({ keys: ['pane:service:a'], gen: 0, provisional: false }, s), false);
});

test('apply refreshes changed fragments and bumps the others', () => {
  const grid = frag('services', 4, true, '/configuration/services', 'services-grid');
  const pane = frag('pane:service:a guides', 4, false, '/configuration/option/a', 'options-pane');
  const d = deps(false);
  const out = S.apply(doc([grid, pane]), sync(5, { 'pane:service:a': 5, services: 5 }, {}), d);
  assert.deepEqual(out, ['/configuration/option/a']);
  assert.deepEqual(d.calls, ['/configuration/option/a']);
  // Unchanged (as predicted): takes the generation, no longer provisional.
  assert.equal(grid.attrs['data-neo-eval-gen'], '5');
  assert.equal(grid.attrs['data-neo-eval-provisional'], 'false');
  // Marked so a second push does not fetch again before the response.
  assert.equal(pane.attrs['data-neo-eval-gen'], '5');
  assert.deepEqual(S.apply(doc([pane]), sync(5, { 'pane:service:a': 5 }, {}), d), []);
});

test('apply keeps provisional while another evaluation is pending', () => {
  const grid = frag('services', 4, true, '/configuration/services');
  S.apply(doc([grid]), sync(5, {}, {}, { pending: true }), deps(false));
  assert.equal(grid.attrs['data-neo-eval-gen'], '5');
  assert.equal(grid.attrs['data-neo-eval-provisional'], 'true');
});

test('apply never replaces a pane with unsaved edits', () => {
  const pane = frag('pane:service:a', 4, false, '/configuration/option/a', 'options-pane');
  let inserted = null;
  pane.insertBefore = (n) => {
    inserted = n;
  };
  pane.firstChild = null;
  const d = deps(true);
  const fakeDoc = doc([pane]);
  global.document = {
    createElement: (tag) => {
      const n = el({});
      n.tagName = tag.toUpperCase();
      n.children = [];
      n.appendChild = (c) => n.children.push(c);
      n.addEventListener = () => {};
      return n;
    },
  };
  try {
    assert.deepEqual(S.apply(fakeDoc, sync(5, { 'pane:service:a': 5 }, {}), d), []);
  } finally {
    delete global.document;
  }
  assert.deepEqual(d.calls, []);
  assert.ok(inserted, 'a note was added');
  assert.equal(pane.attrs['data-neo-eval-gen'], '4');
});

test('apply waits while the user types in a fragment, and sets the theme', () => {
  const input = { tagName: 'INPUT' };
  const grid = frag('services', 4, false, '/configuration/services');
  grid.contains = (n) => n === input;
  const d = deps(false);
  const fakeDoc = doc([grid], { active: input });
  assert.deepEqual(S.apply(fakeDoc, sync(5, { services: 5 }, {}, { theme: 'dark' }), d), []);
  assert.equal(typeof grid.listeners.focusout, 'function');
  assert.equal(fakeDoc.body.attrs['data-theme'], 'dark');
});
