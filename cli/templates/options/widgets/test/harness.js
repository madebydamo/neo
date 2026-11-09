// Shared test host for colocated widget scripts.
// Widgets register on globalThis.NeoWidgets; this runs them in the real
// option_form.js, without Alpine or a browser.
'use strict';

const fs = require('fs');
const path = require('path');
const vm = require('vm');

const optionFormPath = path.join(__dirname, '../../../../static/option_form.js');

function cloneValue(v) {
  return v === undefined ? v : JSON.parse(JSON.stringify(v));
}

function createFakeElement(id, extras) {
  extras = extras || {};
  const listeners = new Map();
  const el = {
    id,
    innerHTML: extras.innerHTML || '',
    textContent: extras.textContent != null ? extras.textContent : '',
    className: extras.className || '',
    open: false,
    value: extras.value || '',
    dataset: extras.dataset || {},
    addEventListener(type, fn) {
      if (!listeners.has(type)) listeners.set(type, new Set());
      listeners.get(type).add(fn);
    },
    removeEventListener(type, fn) {
      listeners.get(type)?.delete(fn);
    },
    dispatchEvent(ev) {
      const type = typeof ev === 'string' ? ev : ev.type;
      for (const fn of listeners.get(type) || []) fn(ev);
    },
    showModal() { el.open = true; },
    close() {
      el.open = false;
      el.dispatchEvent({ type: 'close' });
    },
    click() { el.dispatchEvent({ type: 'click' }); },
  };
  return el;
}

function createDocument(seed) {
  seed = seed || {};
  const elements = new Map();
  const doc = {
    elements,
    addEventListener() {},
    getElementById(id) {
      if (elements.has(id)) return elements.get(id);
      return null;
    },
    set(id, extras) {
      const el = createFakeElement(id, extras);
      elements.set(id, el);
      return el;
    },
  };
  if (seed.pluginInventory != null) {
    doc.set('plugin-inventory-seed', {
      textContent: typeof seed.pluginInventory === 'string'
        ? seed.pluginInventory
        : JSON.stringify(seed.pluginInventory),
    });
  }
  if (seed.pane) {
    doc.set('options-pane', {
      dataset: {
        service: seed.pane.service || '',
        isCore: seed.pane.isCore ? 'true' : 'false',
        saveEndpoint: seed.pane.saveEndpoint || '',
      },
    });
  }
  return doc;
}

function installGlobals(opts) {
  opts = opts || {};
  const document = opts.document || createDocument(opts);
  globalThis.document = document;
  if (!globalThis.window) globalThis.window = globalThis;
  globalThis.window.confirm = opts.confirm || (() => true);
  globalThis.window.neoToast = opts.neoToast || (() => {});
  globalThis.window.NeoUtil = require('../../../../static/neo_util.js');
  if (opts.fetch) {
    globalThis.fetch = opts.fetch;
  }
  return document;
}

function loadOptionForm() {
  if (typeof globalThis.optionForm === 'function') return;
  vm.runInThisContext(fs.readFileSync(optionFormPath, 'utf8'), { filename: 'option_form.js' });
}

/**
 * The real optionForm(), seeded like initForm does. `values` / `defaults` /
 * `originals` override the seeded state of single options.
 */
function makeForm(opts) {
  opts = opts || {};
  const document = installGlobals(opts);
  loadOptionForm();
  const form = globalThis.optionForm();
  form.document = document;
  form.serviceName = opts.serviceName || 'svc';
  form.isCore = !!opts.isCore;
  form.$watch = opts.$watch || null;
  if (opts.pluginInv !== undefined) form._pluginInv = opts.pluginInv;

  form.seedOptions(opts.options || []);
  Object.assign(form.values, cloneValue(opts.values));
  Object.assign(form.defaults, cloneValue(opts.defaults));
  Object.keys(form.optionsByName).forEach((name) => {
    form.originals[name] = cloneValue(form.values[name]);
  });
  Object.assign(form.originals, cloneValue(opts.originals));
  return form;
}

module.exports = {
  cloneValue,
  createDocument,
  createFakeElement,
  installGlobals,
  makeForm,
};
