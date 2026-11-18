'use strict';
const test = require('node:test');
const assert = require('node:assert');
const NeoUtil = require('./neo_util.js');

test('esc escapes markup and treats null as empty', () => {
  assert.strictEqual(NeoUtil.esc(`<a href="x" title='y'>&`), '&lt;a href=&quot;x&quot; title=&#39;y&#39;&gt;&amp;');
  assert.strictEqual(NeoUtil.esc(null), '');
  assert.strictEqual(NeoUtil.esc(undefined), '');
  assert.strictEqual(NeoUtil.esc(0), '0');
});

test('toast does nothing without a host', () => {
  globalThis.document = { getElementById: () => null };
  try {
    NeoUtil.toast('hello', 'error');
  } finally {
    delete globalThis.document;
  }
});

function fakeWin() {
  const data = new Map();
  return {
    localStorage: {
      getItem: (k) => (data.has(k) ? data.get(k) : null),
      setItem: (k, v) => data.set(k, v),
      removeItem: (k) => data.delete(k),
    },
  };
}

test('store round-trips text and JSON', () => {
  const s = NeoUtil.store('local', fakeWin());
  assert.strictEqual(s.get('a'), null);
  s.set('a', '1');
  assert.strictEqual(s.get('a'), '1');
  s.remove('a');
  assert.strictEqual(s.get('a'), null);
  assert.deepStrictEqual(s.getJson('j', { none: true }), { none: true });
  s.setJson('j', { x: 1 });
  assert.deepStrictEqual(s.getJson('j', null), { x: 1 });
  s.set('bad', '{');
  assert.strictEqual(s.getJson('bad', 'fallback'), 'fallback');
});

test('store survives storage that throws', () => {
  const boom = () => {
    throw new Error('blocked');
  };
  const s = NeoUtil.store('local', { localStorage: { getItem: boom, setItem: boom, removeItem: boom } });
  assert.strictEqual(s.get('a'), null);
  assert.strictEqual(s.getJson('a', 7), 7);
  s.set('a', '1');
  s.setJson('a', 1);
  s.remove('a');
  // No storage at all (access itself fails).
  const none = NeoUtil.store('session', {});
  assert.strictEqual(none.get('a'), null);
  none.set('a', '1');
});

test('onReady waits for a loading document and runs at once otherwise', () => {
  let ran = 0;
  let listener = null;
  NeoUtil.onReady({ readyState: 'loading', addEventListener: (ev, fn) => { assert.strictEqual(ev, 'DOMContentLoaded'); listener = fn; } }, () => ran++);
  assert.strictEqual(ran, 0);
  listener();
  assert.strictEqual(ran, 1);
  NeoUtil.onReady({ readyState: 'complete' }, () => ran++);
  assert.strictEqual(ran, 2);
});

test('reloadPane loads the pane URL into the content area', () => {
  const calls = [];
  globalThis.htmx = { ajax: (...args) => calls.push(args) };
  globalThis.document = { getElementById: () => ({ dataset: { loadUrl: '/configuration/option/a' } }) };
  try {
    assert.strictEqual(NeoUtil.reloadPane(), true);
    assert.deepStrictEqual(calls, [['GET', '/configuration/option/a', { target: '#config-content', swap: 'innerHTML' }]]);
    globalThis.document = { getElementById: () => null };
    assert.strictEqual(NeoUtil.reloadPane(), false);
  } finally {
    delete globalThis.htmx;
    delete globalThis.document;
  }
});

test('readJson returns the body and rejects with the best message', async () => {
  const res = (ok, status, json) => ({ ok, status, json });
  assert.deepStrictEqual(await NeoUtil.readJson(res(true, 200, async () => ({ a: 1 }))), { a: 1 });
  await assert.rejects(NeoUtil.readJson(res(false, 500, async () => ({ error: 'nope' }))), /^Error: nope$/);
  await assert.rejects(NeoUtil.readJson(res(false, 500, async () => ({})), 'Broken'), /^Error: Broken \(500\)\.$/);
  const notJson = async () => {
    throw new SyntaxError('Unexpected token <');
  };
  await assert.rejects(NeoUtil.readJson(res(false, 502, notJson)), /^Error: Request failed \(502\)\.$/);
  await assert.rejects(NeoUtil.readJson(res(true, 200, notJson), 'Broken'), /^Error: Broken \(200\)\.$/);
});

test('isContentSwap: the content area, or the pane inside it', () => {
  assert.strictEqual(NeoUtil.isContentSwap({ id: 'config-content' }), true);
  assert.strictEqual(NeoUtil.isContentSwap({ id: 'options-pane', closest: () => ({}) }), true);
  assert.strictEqual(NeoUtil.isContentSwap({ id: 'options-pane', closest: () => null }), false);
  assert.strictEqual(NeoUtil.isContentSwap({ id: 'other', closest: () => ({}) }), false);
});

test('h sets classes, text, attributes, listeners, properties and children', () => {
  const made = [];
  globalThis.document = {
    createElement: (tag) => {
      const el = {
        tag,
        attrs: {},
        kids: [],
        listeners: {},
        setAttribute(k, v) { this.attrs[k] = v; },
        addEventListener(k, f) { this.listeners[k] = f; },
        appendChild(c) { this.kids.push(c); },
      };
      made.push(el);
      return el;
    },
  };
  try {
    const onClick = () => {};
    const kid = NeoUtil.h('i');
    const el = NeoUtil.h('button', { class: 'a b', text: 'go', title: 't', disabled: true, attrs: { role: 'x' }, on: { click: onClick } }, kid, null, false);
    assert.strictEqual(el.className, 'a b');
    assert.strictEqual(el.textContent, 'go');
    assert.strictEqual(el.title, 't');
    assert.strictEqual(el.disabled, true);
    assert.strictEqual(el.attrs.role, 'x');
    assert.strictEqual(el.listeners.click, onClick);
    assert.deepStrictEqual(el.kids, [kid]);
  } finally {
    delete globalThis.document;
  }
});

/** A document with the click listener wireHooks adds, and elements by id. */
function hookDoc(byId) {
  const listeners = {};
  const timers = [];
  const copied = [];
  const doc = {
    addEventListener: (type, fn) => { listeners[type] = fn; },
    getElementById: (id) => byId[id] || null,
    defaultView: {
      navigator: { clipboard: { writeText: (t) => { copied.push(t); return Promise.resolve(); } } },
      setTimeout: (fn) => timers.push(fn),
    },
  };
  NeoUtil.wireHooks(doc);
  const click = (target) => listeners.click({ target, preventDefault() {} });
  return { click, timers, copied };
}

/** An element whose closest() finds the first of `chain` matching the selector. */
function el(attrs, chain) {
  const self = {
    attrs,
    textContent: attrs.text || '',
    removed: false,
    getAttribute: (k) => (k in attrs ? attrs[k] : null),
    remove() { self.removed = true; },
    closest(sel) {
      for (const c of [self, ...(chain || [])]) {
        const m = /^\[([\w-]+)\]$/.exec(sel);
        if (m ? m[1] in c.attrs : c.attrs.tag === sel) return c;
      }
      return null;
    },
  };
  return self;
}

test('wireHooks copies an element\'s text and restores the label', async () => {
  const key = { textContent: '  ssh-ed25519 AAAA host \n' };
  const button = el({ 'data-neo-copy': 'key', text: 'Copy' });
  const { click, timers, copied } = hookDoc({ key });
  click(button);
  await Promise.resolve();
  assert.deepStrictEqual(copied, ['ssh-ed25519 AAAA host']);
  assert.strictEqual(button.textContent, 'Copied');
  // A second click meanwhile keeps the original label.
  click(button);
  await Promise.resolve();
  timers.forEach((f) => f());
  assert.strictEqual(button.textContent, 'Copy');
});

test('wireHooks removes the closest match of data-neo-remove-closest', () => {
  const form = el({ tag: 'form' });
  const cancel = el({ 'data-neo-remove-closest': 'form' }, [form]);
  const { click } = hookDoc({});
  click(cancel);
  assert.strictEqual(form.removed, true);
  assert.strictEqual(cancel.removed, false);
});

test('runChecks: true/false at once, a Promise once a check is async', async () => {
  assert.strictEqual(NeoUtil.runChecks([() => true, () => undefined]), true);
  assert.strictEqual(NeoUtil.runChecks([() => true, () => false, () => { throw new Error('ran'); }]), false);
  const order = [];
  const r = NeoUtil.runChecks([
    () => Promise.resolve(true).then((v) => (order.push('a'), v)),
    () => (order.push('b'), Promise.resolve(false)),
    () => { throw new Error('ran after a refusal'); },
  ]);
  assert.ok(r && typeof r.then === 'function');
  assert.strictEqual(await r, false);
  assert.deepStrictEqual(order, ['a', 'b']);
});

test('confirmLabel: data-neo-confirm-ok, short text, then aria-label', () => {
  const el = (attrs, text) => ({ getAttribute: (k) => (k in attrs ? attrs[k] : null), textContent: text });
  assert.strictEqual(NeoUtil.confirmLabel(el({ 'data-neo-confirm-ok': 'Wipe' }, 'Clear')), 'Wipe');
  assert.strictEqual(NeoUtil.confirmLabel(el({}, '\n  Shut   down \n')), 'Shut down');
  assert.strictEqual(NeoUtil.confirmLabel(el({ 'aria-label': 'Delete' }, '')), 'Delete');
  assert.strictEqual(NeoUtil.confirmLabel(null), 'OK');
});

test('htmx:confirm: guards hold the request and send it once they agree', async () => {
  const handlers = {};
  const doc = { addEventListener: (type, fn) => { handlers[type] = fn; } };
  NeoUtil.wireHooks(doc);
  let answer;
  NeoUtil.guardRequests((d) => (d.path === '/guarded' ? new Promise((res) => { answer = res; }) : true));

  const fire = (path) => {
    const evt = { prevented: false, sent: null, preventDefault() { this.prevented = true; } };
    evt.detail = { path, question: null, issueRequest: (skip) => { evt.sent = skip; } };
    handlers['htmx:confirm'](evt);
    return evt;
  };

  const free = fire('/free');
  assert.strictEqual(free.prevented, false);

  const held = fire('/guarded');
  assert.strictEqual(held.prevented, true);
  assert.strictEqual(held.sent, null);
  answer(true);
  await new Promise((r) => setTimeout(r, 0));
  assert.strictEqual(held.sent, true);

  const refused = fire('/guarded');
  answer(false);
  await new Promise((r) => setTimeout(r, 0));
  assert.strictEqual(refused.sent, null);
});
