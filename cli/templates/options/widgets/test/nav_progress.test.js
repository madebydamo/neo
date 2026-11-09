'use strict';

// static/nav_progress.js: generic htmx loading feedback (progress bar, busy
// source + target). Runs under `just test-widgets` with a tiny fake DOM.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const path = require('path');

const NeoNavProgress = require(path.join(__dirname, '../../../../static/nav_progress.js'));

function fakeEl(id, attrs) {
  const a = Object.assign({}, attrs || {});
  const classes = new Set();
  const el = {
    id: id || '',
    nodeType: 1,
    style: {},
    children: [],
    parentNode: null,
    get firstChild() { return this.children[0] || null; },
    className: '',
    classList: {
      add: (...c) => c.forEach((x) => classes.add(x)),
      remove: (...c) => c.forEach((x) => classes.delete(x)),
      contains: (c) => classes.has(c),
    },
    getAttribute: (k) => (k in a ? a[k] : null),
    setAttribute: (k, v) => { a[k] = String(v); },
    removeAttribute: (k) => { delete a[k]; },
    hasAttribute: (k) => k in a,
    appendChild(c) { c.parentNode = this; this.children.push(c); return c; },
    closest(sel) {
      const m = /^\[([\w-]+)\]$/.exec(sel);
      let n = this;
      while (n) {
        if (m && n.hasAttribute && n.hasAttribute(m[1])) return n;
        n = n.parentNode;
      }
      return null;
    },
  };
  return el;
}

function fakeDoc(els) {
  const body = fakeEl('body');
  const byId = {};
  (els || []).forEach((e) => { byId[e.id] = e; });
  return {
    body,
    getElementById: (id) => byId[id] || body.children.find((c) => c.id === id) || null,
    createElement: () => fakeEl(),
    querySelector: (sel) => (sel[0] === '#' ? byId[sel.slice(1)] || null : null),
    // Minimal support for the `[attr], [attr2]` selectors scrubLoadingMarkers uses.
    querySelectorAll: (sel) => {
      const attrs = sel.split(',').map((s) => {
        const m = /^\s*\[([\w-]+)\]\s*$/.exec(s);
        return m ? m[1] : null;
      });
      return Object.keys(byId)
        .map((id) => byId[id])
        .filter((el) => attrs.some((a) => a && el.hasAttribute(a)));
    },
  };
}

function fakeTimers() {
  let now = 0;
  let seq = 0;
  const pending = new Map();
  return {
    setTimeout: (fn, ms) => { const id = ++seq; pending.set(id, { fn, at: now + ms }); return id; },
    clearTimeout: (id) => { pending.delete(id); },
    advance(ms) {
      const end = now + ms;
      for (;;) {
        let next = null;
        for (const [id, t] of pending) if (t.at <= end && (!next || t.at < next[1].at)) next = [id, t];
        if (!next) break;
        pending.delete(next[0]);
        now = next[1].at;
        next[1].fn();
      }
      now = end;
    },
  };
}

function setup() {
  const navBusy = fakeEl('nav-busy');
  const content = fakeEl('config-content');
  const card = fakeEl('card');
  const doc = fakeDoc([navBusy, content, card]);
  const timers = fakeTimers();
  const ctl = NeoNavProgress.createController({ doc, ...timers });
  return { doc, timers, ctl, navBusy, content, card };
}

test('marks source + target immediately, bar only after the show delay', () => {
  const { doc, timers, ctl, content, card } = setup();
  ctl.start('x1', { source: card, target: content });
  assert.equal(card.getAttribute('data-neo-loading'), '1');
  assert.equal(card.getAttribute('aria-busy'), 'true');
  assert.equal(content.getAttribute('aria-busy'), 'true');
  assert.ok(content.classList.contains('neo-swap-busy'));
  assert.equal(ctl.isVisible(), false);
  timers.advance(149);
  assert.equal(ctl.isVisible(), false);
  timers.advance(1);
  assert.equal(ctl.isVisible(), true);
  const bar = doc.getElementById('neo-progress');
  assert.ok(bar.classList.contains('is-active'));
  const p0 = ctl.progress();
  timers.advance(1000);
  assert.ok(ctl.progress() > p0, 'trickles');
  assert.ok(ctl.progress() < 1);
});

test('fast request never shows the bar and cleans up', () => {
  const { timers, ctl, content, card } = setup();
  ctl.start('x1', { source: card, target: content });
  timers.advance(80);
  ctl.end('x1');
  timers.advance(500);
  assert.equal(ctl.isVisible(), false);
  assert.equal(card.getAttribute('data-neo-loading'), null);
  assert.equal(card.getAttribute('aria-busy'), null);
  assert.equal(content.getAttribute('aria-busy'), null);
  assert.ok(!content.classList.contains('neo-swap-busy'));
});

test('slow request adds the Loading label, cleared on end', () => {
  const { doc, timers, ctl, navBusy, content } = setup();
  ctl.start('x1', { target: content });
  timers.advance(1499);
  assert.ok(!navBusy.classList.contains('is-slow'));
  timers.advance(1);
  assert.ok(navBusy.classList.contains('is-slow'));
  ctl.end('x1');
  assert.ok(!navBusy.classList.contains('is-slow'));
  assert.equal(ctl.progress(), 1);
  timers.advance(400);
  assert.equal(ctl.isVisible(), false);
  assert.ok(!doc.getElementById('neo-progress').classList.contains('is-active'));
});

test('overlapping requests: indicator ends only after the last one', () => {
  const { timers, ctl, content } = setup();
  ctl.start('a', { target: content });
  ctl.start('b', { target: content });
  timers.advance(200);
  ctl.end('a');
  assert.equal(content.getAttribute('aria-busy'), 'true');
  assert.equal(ctl.activeCount(), 1);
  ctl.end('b');
  ctl.end('b'); // duplicate end (responseError + afterRequest) is harmless
  assert.equal(content.getAttribute('aria-busy'), null);
  assert.equal(ctl.activeCount(), 0);
});

test('safety timeout always ends a request that never reports back', () => {
  const { timers, ctl, content } = setup();
  ctl.start('lost', { target: content });
  timers.advance(60000);
  assert.equal(ctl.activeCount(), 0);
  assert.equal(content.getAttribute('aria-busy'), null);
});

test('isTracked: config-content swaps and opt-in only', () => {
  const content = fakeEl('config-content');
  const other = fakeEl('changes-body');
  const doc = fakeDoc([content, other]);
  const btn = fakeEl('b');
  assert.equal(NeoNavProgress.isTracked(doc, { elt: btn, target: content }), true);
  assert.equal(NeoNavProgress.isTracked(doc, { elt: btn, target: '#config-content' }), true);
  assert.equal(NeoNavProgress.isTracked(doc, { elt: btn, target: other }), false);
  const optIn = fakeEl('o', { 'data-neo-progress': '' });
  const child = fakeEl('c');
  optIn.appendChild(child);
  assert.equal(NeoNavProgress.isTracked(doc, { elt: child, target: other }), true);
  const optOut = fakeEl('p', { 'data-neo-progress': 'false' });
  assert.equal(NeoNavProgress.isTracked(doc, { elt: optOut, target: content }), false);
});

test('scrubLoadingMarkers clears busy markers the controller no longer tracks', () => {
  // Simulates the exact bug: htmx snapshotted this DOM (for its history cache)
  // while a card/pane were still mid-request, so the markers are "orphaned" —
  // no in-flight controller entry owns them anymore, e.g. after a Back restores
  // that snapshot verbatim.
  const bar = fakeEl('neo-progress');
  const fill = fakeEl();
  bar.appendChild(fill);
  bar.classList.add('is-active', 'is-slow');
  const navBusy = fakeEl('nav-busy');
  navBusy.classList.add('is-slow');
  const card = fakeEl('card', { 'data-neo-loading': '1', 'aria-busy': 'true' });
  const content = fakeEl('content', { 'data-neo-busy-count': '1', 'aria-busy': 'true' });
  content.classList.add('neo-swap-busy');
  const doc = fakeDoc([bar, navBusy, card, content]);

  NeoNavProgress.scrubLoadingMarkers(doc);

  assert.equal(card.getAttribute('data-neo-loading'), null);
  assert.equal(card.getAttribute('aria-busy'), null);
  assert.equal(content.getAttribute('data-neo-busy-count'), null);
  assert.equal(content.getAttribute('aria-busy'), null);
  assert.ok(!content.classList.contains('neo-swap-busy'));
  assert.ok(!bar.classList.contains('is-active'));
  assert.ok(!bar.classList.contains('is-slow'));
  assert.equal(fill.style.transform, 'scaleX(0)');
  assert.ok(!navBusy.classList.contains('is-slow'));
});

test('scrubLoadingMarkers is a harmless no-op when there is nothing stale', () => {
  const doc = fakeDoc([]);
  assert.doesNotThrow(() => NeoNavProgress.scrubLoadingMarkers(doc));
});

test('onBusyChange reports navigations only, once per change', () => {
  const content = fakeEl('config-content');
  const other = fakeEl('other');
  const doc = fakeDoc([content, other]);
  const timers = fakeTimers();
  const seen = [];
  const ctl = NeoNavProgress.createController({ doc, ...timers, onBusyChange: (b) => seen.push(b) });
  ctl.start('opted-in', { target: other });
  assert.deepEqual(seen, []);
  ctl.start('nav1', { target: content });
  ctl.start('nav2', { target: content });
  assert.deepEqual(seen, [true]);
  ctl.end('nav1');
  assert.deepEqual(seen, [true]);
  ctl.end('nav2');
  assert.deepEqual(seen, [true, false]);
  ctl.end('opted-in');
  assert.deepEqual(seen, [true, false]);
});
