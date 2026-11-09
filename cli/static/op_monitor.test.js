'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const m = require('./op_monitor.js');

test('backoff grows and caps', () => {
  assert.equal(m.backoff(0), 300);
  assert.ok(m.backoff(2) > m.backoff(1));
  assert.equal(m.backoff(50), m.backoff(4));
});

test('wsUrl picks scheme and carries the resume offset', () => {
  assert.equal(
    m.wsUrl({ protocol: 'https:', host: 'neo.example' }, 'activation_1', 42),
    'wss://neo.example/ws/op/activation_1?offset=42'
  );
  assert.equal(m.wsUrl({ protocol: 'http:', host: 'h:1' }, 'update_x'), 'ws://h:1/ws/op/update_x?offset=0');
});

test('stepStates: running, failed and success', () => {
  assert.deepEqual(m.stepStates(4, 2, 'in_progress'), ['done', 'done', 'current', 'pending']);
  assert.deepEqual(m.stepStates(3, 1, 'failed'), ['done', 'failed', 'pending']);
  assert.deepEqual(m.stepStates(3, 2, 'success'), ['done', 'done', 'done']);
});

test('statusView tones', () => {
  assert.equal(m.statusView('in_progress').tone, 'running');
  assert.equal(m.statusView('success').tone, 'success');
  assert.equal(m.statusView('failed').tone, 'error');
  assert.equal(m.statusView('').label, 'Connecting');
});

test('outcome: activation success offers reload; update does not', () => {
  const a = m.outcome({ kind: 'activation', status: 'success', branch: 'act-1' });
  assert.equal(a.reload, true);
  assert.match(a.text, /act-1/);
  assert.ok(!m.outcome({ kind: 'update', status: 'success' }).reload);
  assert.equal(m.outcome({ kind: 'update', status: 'failed', error: 'boom' }).text, 'boom');
  assert.equal(m.outcome({ status: 'in_progress' }), null);
});

test('humanPhase', () => {
  assert.equal(m.humanPhase('toplevel-build'), 'toplevel build');
});

test('log pane markup matches the activation output, with an optional Clear', () => {
  const html = m.logPane.markup();
  assert.match(html, /class="neo-op-output"/);
  assert.match(html, /data-el="log"/);
  assert.match(html, /data-act="copy"/);
  assert.match(html, /data-act="jump"[^>]*>[\s\S]*Latest/);
  assert.match(html, /hidden/);
  assert.doesNotMatch(html, /data-act="clear"/);
  assert.match(m.logPane.markup({ clear: true }), /data-act="clear"/);
});

test('nearBottom uses the same 24px slack as the activation log', () => {
  assert.equal(m.logPane.nearBottom({ scrollHeight: 200, scrollTop: 77, clientHeight: 100 }), true);
  assert.equal(m.logPane.nearBottom({ scrollHeight: 200, scrollTop: 76, clientHeight: 100 }), false);
});

/** Minimal element tree so logPane.bind can run without a browser. */
function node(attrs, kids) {
  const children = [];
  const listeners = {};
  const classes = new Set(String(attrs.class || '').split(/\s+/).filter(Boolean));
  const el = {
    nodeType: 1,
    hidden: !!attrs.hidden,
    children,
    parentNode: null,
    scrollHeight: 0,
    scrollTop: 0,
    clientHeight: 0,
    _attrs: Object.assign({}, attrs),
    _text: '',
    classList: {
      add: (c) => classes.add(c),
      remove: (c) => classes.delete(c),
      contains: (c) => classes.has(c),
    },
  };
  Object.defineProperty(el, 'textContent', {
    get() {
      return children.length ? children.map((c) => c.textContent).join('') : el._text;
    },
    set(v) {
      children.splice(0, children.length);
      el._text = String(v);
    },
  });
  if (attrs.text) el.textContent = attrs.text;
  el.getAttribute = (k) => (k in el._attrs ? el._attrs[k] : null);
  el.appendChild = (child) => {
    child.parentNode = el;
    children.push(child);
    return child;
  };
  el.removeChild = (child) => {
    const i = children.indexOf(child);
    if (i >= 0) children.splice(i, 1);
    child.parentNode = null;
    return child;
  };
  Object.defineProperty(el, 'firstChild', { get() { return children[0] || null; } });
  Object.defineProperty(el, 'lastChild', { get() { return children[children.length - 1] || null; } });
  el.addEventListener = (type, fn) => {
    (listeners[type] || (listeners[type] = [])).push(fn);
  };
  el.dispatch = (type, event) => {
    (listeners[type] || []).forEach((fn) => fn(event || {}));
  };
  el.contains = (n) => {
    if (!n) return false;
    if (n === el) return true;
    return children.some((c) => c === n || (typeof c.contains === 'function' && c.contains(n)));
  };
  el.querySelector = (sel) => {
    const found = [];
    (function walk(n) {
      found.push(n);
      n.children.forEach(walk);
    })(el);
    const match = sel.match(/^\[([\w-]+)="([^"]+)"\]$/);
    return found.find((n) => n.getAttribute && n.getAttribute(match[1]) === match[2]) || null;
  };
  (kids || []).forEach((kid) => el.appendChild(kid));
  return el;
}

function outputTree() {
  const lines = node({ 'data-el': 'lines' });
  const trimmed = node({ 'data-el': 'trimmed', class: 'opacity-60 hidden' });
  const copy = node({ 'data-act': 'copy', text: 'Copy' });
  const clear = node({ 'data-act': 'clear', text: 'Clear' });
  const log = node({ 'data-el': 'log' });
  const jump = node({ 'data-act': 'jump', hidden: true, text: 'Latest' });
  const root = node({ class: 'neo-op-output' }, [
    node({ class: 'neo-op-output-bar' }, [lines, trimmed, clear, copy]),
    node({ class: 'neo-op-log-wrap' }, [log, jump]),
  ]);
  return { root, log, jump, lines, trimmed, copy, clear };
}

function fakeWin() {
  const timeouts = [];
  let written = null;
  let selection = null;
  const win = {
    timeouts,
    setTimeout(fn) { timeouts.push(fn); return timeouts.length; },
    navigator: {
      clipboard: {
        writeText(text) {
          written = text;
          return Promise.resolve();
        },
      },
    },
    get written() { return written; },
    getSelection() { return selection; },
    setSelection(sel) { selection = sel; },
    document: {
      createTextNode(text) {
        return { nodeType: 3, textContent: String(text), parentNode: null, children: [], contains() { return false; } };
      },
    },
  };
  return win;
}

test('log pane follows the tail and offers Latest when scrolled away', () => {
  const ui = outputTree();
  const win = fakeWin();
  const pane = m.logPane.bind(ui.root, win);
  ui.log.scrollHeight = 400;
  ui.log.clientHeight = 100;
  pane.append('hello\n');
  assert.equal(ui.log.scrollTop, 400);
  assert.equal(ui.jump.hidden, true);
  assert.equal(ui.lines.textContent, '1 line');

  ui.log.scrollTop = 0;
  ui.log.dispatch('scroll');
  assert.equal(ui.jump.hidden, false);
  pane.append('more\n');
  assert.equal(ui.log.scrollTop, 0);
  assert.equal(ui.lines.textContent, '2 lines');

  ui.jump.dispatch('click');
  assert.equal(ui.log.scrollTop, 400);
  assert.equal(ui.jump.hidden, true);
});

test('log pane does not steal the scroll while a selection is inside the output', () => {
  const ui = outputTree();
  const win = fakeWin();
  const pane = m.logPane.bind(ui.root, win);
  ui.log.scrollHeight = 400;
  ui.log.clientHeight = 100;
  pane.append('hello\n');
  win.setSelection({ isCollapsed: false, anchorNode: ui.log.firstChild });
  ui.log.scrollTop = 12;
  pane.append('more\n');
  assert.equal(ui.log.scrollTop, 12);
});

test('log pane trims old output, copies, and Clear restores follow', async () => {
  const ui = outputTree();
  const win = fakeWin();
  const pane = m.logPane.bind(ui.root, win, { maxChars: 8 });
  pane.append('01234567');
  pane.append('WXYZ');
  assert.equal(ui.log.textContent, 'WXYZ');
  assert.equal(ui.trimmed.classList.contains('hidden'), false);

  ui.log.scrollTop = 0;
  ui.log.scrollHeight = 100;
  ui.log.clientHeight = 10;
  ui.log.dispatch('scroll');
  pane.clear();
  assert.equal(ui.log.textContent, '');
  assert.equal(ui.lines.textContent, '');
  assert.equal(ui.jump.hidden, true);
  assert.equal(ui.trimmed.classList.contains('hidden'), true);
  ui.log.scrollHeight = 50;
  pane.append('b\n');
  assert.equal(ui.log.scrollTop, 50);

  ui.copy.dispatch('click', { currentTarget: ui.copy });
  await Promise.resolve();
  assert.equal(win.written, 'b\n');
  assert.equal(ui.copy.textContent, 'Copied');
  win.timeouts[0]();
  assert.equal(ui.copy.textContent, 'Copy');
});

test('log pane discard drops text without jumping back to follow', () => {
  const ui = outputTree();
  const pane = m.logPane.bind(ui.root, fakeWin());
  ui.log.scrollHeight = 100;
  ui.log.clientHeight = 10;
  pane.append('kept\n');
  ui.log.scrollTop = 0;
  ui.log.dispatch('scroll');
  pane.discard();
  assert.equal(ui.log.textContent, '');
  assert.equal(ui.jump.hidden, false);
});

function changesDom({ open = false, bodyHtml = '' } = {}) {
  const heading = { textContent: '' };
  const modal = { open, shown: 0, querySelector: () => heading, showModal() { this.shown++; this.open = true; } };
  const body = { innerHTML: bodyHtml };
  const doc = { getElementById: (id) => (id === 'changes-modal' ? modal : id === 'changes-body' ? body : null) };
  return { heading, modal, body, doc };
}

test('changesAfterRequest opens the dialog only after success', () => {
  const dom = changesDom();
  globalThis.document = dom.doc;
  try {
    m.changesAfterRequest({ detail: { successful: false } }, 'Update');
    assert.equal(dom.modal.shown, 0);
    m.changesAfterRequest({ detail: { successful: true } }, 'Update');
    assert.equal(dom.modal.shown, 1);
    assert.equal(dom.heading.textContent, 'Update');
    // Already open: showModal would throw on a real dialog.
    m.changesAfterRequest({ detail: { successful: true } }, 'Again');
    assert.equal(dom.modal.shown, 1);
    assert.equal(dom.heading.textContent, 'Again');
  } finally {
    delete globalThis.document;
  }
});

test('changesAfterRequest with ifFilled needs output in the body', () => {
  const dom = changesDom({ bodyHtml: '  ' });
  globalThis.document = dom.doc;
  try {
    m.changesAfterRequest({ detail: { successful: true } }, 'Activation', true);
    assert.equal(dom.modal.shown, 0);
    dom.body.innerHTML = '<div></div>';
    m.changesAfterRequest({ detail: { successful: true } }, 'Activation', true);
    assert.equal(dom.modal.shown, 1);
  } finally {
    delete globalThis.document;
  }
});

test('openChanges replaces the body when given content', () => {
  const dom = changesDom({ bodyHtml: 'old' });
  globalThis.document = dom.doc;
  try {
    m.openChanges('T');
    assert.equal(dom.body.innerHTML, 'old');
    m.openChanges('T', 'new');
    assert.equal(dom.body.innerHTML, 'new');
  } finally {
    delete globalThis.document;
  }
});
