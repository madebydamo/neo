'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const Nav = require('./neo.js');

test('keyFor: the neo entry uses the config iframe key', () => {
  assert.equal(Nav.keyFor('neo'), '__config');
  assert.equal(Nav.keyFor('photos'), 'photos');
});

test('configTargetUrl: absolute on the service subdomain when a domain is known', () => {
  assert.equal(Nav.configTargetUrl('neo', 'home.example'), 'https://neo.home.example/configuration');
  assert.equal(Nav.configTargetUrl('', 'home.example'), 'https://neo.home.example/configuration');
  assert.equal(Nav.configTargetUrl('neo', ''), '/configuration');
});

test('serviceRootUrl: needs subdomain and domain', () => {
  assert.equal(Nav.serviceRootUrl('photos', 'home.example'), 'https://photos.home.example/');
  assert.equal(Nav.serviceRootUrl('photos', ''), null);
  assert.equal(Nav.serviceRootUrl('', 'home.example'), null);
});

test('isModifiedClick: modifier keys and non-primary buttons go to the browser', () => {
  assert.equal(Nav.isModifiedClick({ button: 0 }), false);
  for (const k of ['shiftKey', 'ctrlKey', 'metaKey', 'altKey']) {
    assert.equal(Nav.isModifiedClick({ button: 0, [k]: true }), true, k);
  }
  assert.equal(Nav.isModifiedClick({ button: 1 }), true);
});

test('iframeDisplayUrl: same-origin location, then known URL, then src', () => {
  const sameOrigin = { contentWindow: { location: { href: 'https://a.x/deep' } }, src: 'https://a.x/' };
  assert.equal(Nav.iframeDisplayUrl(sameOrigin, 'https://a.x/known'), 'https://a.x/deep');

  const crossOrigin = {
    get contentWindow() {
      return { get location() { throw new Error('SecurityError'); } };
    },
    src: 'https://a.x/',
  };
  assert.equal(Nav.iframeDisplayUrl(crossOrigin, 'https://a.x/known'), 'https://a.x/known');
  assert.equal(Nav.iframeDisplayUrl(crossOrigin, undefined), 'https://a.x/');

  const blank = { contentWindow: { location: { href: 'about:blank' } }, src: 'about:blank' };
  assert.equal(Nav.iframeDisplayUrl(blank), '');
  assert.equal(Nav.iframeDisplayUrl(null, 'https://a.x/'), '');
});

test('reportedLocation: only well-formed messages from the iframe origin', () => {
  const msg = { source: 'neo-iframe-location', href: 'https://a.x/deep' };
  const base = 'https://nav.x/';
  assert.equal(Nav.reportedLocation(msg, 'https://a.x', 'https://a.x/', base), 'https://a.x/deep');
  assert.equal(Nav.reportedLocation(msg, 'https://evil.x', 'https://a.x/', base), null);
  assert.equal(Nav.reportedLocation({ source: 'other', href: 'x' }, 'https://a.x', 'https://a.x/', base), null);
  assert.equal(Nav.reportedLocation({ source: 'neo-iframe-location', href: 1 }, 'https://a.x', 'https://a.x/', base), null);
  assert.equal(Nav.reportedLocation(null, 'https://a.x', 'https://a.x/', base), null);
  // A relative src resolves against the navigator's own origin.
  assert.equal(Nav.reportedLocation(msg, 'https://nav.x', '/configuration', base), 'https://a.x/deep');
});
