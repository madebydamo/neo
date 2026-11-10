'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { health, dots } = require('./service_status.js');

const S = (entries) => health.summarize(entries);

test('summarize: all active is ok/running', () => {
  assert.deepEqual(S([{ state: 'active' }, { state: 'active' }]), { state: 'ok', text: 'running' });
});

test('summarize: any failed wins (red)', () => {
  assert.deepEqual(S([{ state: 'active' }, { state: 'failed' }]), { state: 'failed', text: '1 failed' });
  assert.equal(S([{ state: 'failed' }, { state: 'failed' }]).text, '2 failed');
  assert.equal(S([{ state: 'activating' }, { state: 'failed' }]).state, 'failed');
});

test('summarize: activating/deactivating is changing (amber)', () => {
  assert.equal(S([{ state: 'active' }, { state: 'activating' }]).state, 'changing');
  assert.equal(S([{ state: 'deactivating' }]).state, 'changing');
});

test('summarize: partially running is partial (amber), all stopped is down (red)', () => {
  assert.deepEqual(S([{ state: 'active' }, { state: 'inactive' }]), { state: 'partial', text: '1/2 running' });
  assert.deepEqual(S([{ state: 'inactive' }, { state: 'inactive' }]), { state: 'down', text: 'stopped' });
});

test('summarize: timers only count when failed', () => {
  assert.equal(S([{ state: 'active' }, { state: 'inactive', timer: true }]).state, 'ok');
  assert.deepEqual(S([{ state: 'inactive', timer: true }]), { state: 'ok', text: 'idle' });
  assert.equal(S([{ state: 'active' }, { state: 'failed', timer: true }]).state, 'failed');
});

test('summarize: pending (pane waiting on WS) is unknown/checking', () => {
  assert.deepEqual(S([{ state: 'active' }, { state: null }]), { state: 'unknown', text: 'checking…' });
  assert.equal(S([{ state: '', timer: true }]).state, 'unknown');
});

test('summarize: unknown / not deployed are grey (none)', () => {
  assert.deepEqual(S([{ state: 'unknown' }]), { state: 'none', text: 'status unknown' });
  assert.deepEqual(
    S([{ state: 'inactive', load: 'not-found' }, { state: 'inactive', load: 'not-found' }]),
    { state: 'none', text: 'not deployed' },
  );
  // Partially missing: the missing unit is simply not running.
  assert.equal(S([{ state: 'active' }, { state: 'inactive', load: 'not-found' }]).state, 'partial');
});

test('summarize: no units is a neutral configured state, not grey', () => {
  assert.deepEqual(S([]), { state: 'idle', text: 'configured · no processes' });
  assert.deepEqual(S(undefined), { state: 'idle', text: 'configured · no processes' });
});

test('summarize: finished setup unit counts as up (no "1/2 running")', () => {
  // collabora: container running + collabora-setup active (exited).
  assert.deepEqual(
    S([
      { state: 'active', health: 'running' },
      { state: 'active', health: 'done' },
    ]),
    { state: 'ok', text: 'running' },
  );
  // Inactive oneshot whose last run succeeded (server says done).
  assert.equal(S([{ state: 'active', health: 'running' }, { state: 'inactive', health: 'done' }]).state, 'ok');
  // A daemon really down still shows partial.
  assert.deepEqual(
    S([{ state: 'active', health: 'running' }, { state: 'inactive', health: 'stopped' }]),
    { state: 'partial', text: '1/2 running' },
  );
});

test('summarize: setup in progress is changing, failed setup is failed', () => {
  assert.deepEqual(
    S([{ state: 'active', health: 'running' }, { state: 'active', health: 'setup' }]),
    { state: 'changing', text: 'setting up…' },
  );
  assert.equal(
    S([{ state: 'activating', health: 'changing' }, { state: 'active', health: 'setup' }]).text,
    'changing…',
  );
  assert.deepEqual(
    S([{ state: 'active', health: 'running' }, { state: 'failed', health: 'failed' }]),
    { state: 'failed', text: '1 failed' },
  );
});

test('summarize: never-run oneshot job (idle) is neutral', () => {
  assert.deepEqual(S([{ state: 'active', health: 'running' }, { state: 'inactive', health: 'idle' }]), {
    state: 'ok',
    text: 'running',
  });
  assert.deepEqual(S([{ state: 'inactive', health: 'idle' }]), { state: 'ok', text: 'idle' });
});

test('healthOf: falls back to ActiveState without server health', () => {
  const H = health.healthOf;
  assert.equal(H({ state: 'active' }), 'running');
  assert.equal(H({ state: 'inactive' }), 'stopped');
  assert.equal(H({ state: 'deactivating' }), 'changing');
  assert.equal(H({ state: 'inactive', load: 'not-found' }), 'missing');
  assert.equal(H({ state: 'failed', load: 'not-found' }), 'failed');
  assert.equal(H({ state: null }), null);
  assert.equal(H({ state: 'active', health: 'done' }), 'done');
});

function fakeEl(attrs) {
  const a = Object.assign({}, attrs);
  return {
    attrs: a,
    getAttribute: (k) => (k in a ? a[k] : null),
    setAttribute: (k, v) => { a[k] = String(v); },
  };
}

function fakeScope(els) {
  return { querySelectorAll: () => els };
}

test('collectRequest: one entry per service, units/timers split on whitespace', () => {
  const els = [
    fakeEl({ 'data-service-status': 'immich', 'data-units': 'docker-immich  docker-immich-db', 'data-timers': '' }),
    fakeEl({ 'data-service-status': 'pihole', 'data-units': 'docker-pihole gravity', 'data-timers': 'gravity' }),
    // Same service in grid + sidebar: requested once.
    fakeEl({ 'data-service-status': 'immich', 'data-units': 'docker-immich' }),
  ];
  const req = dots.collectRequest(fakeScope(els));
  assert.equal(req.count, 2);
  assert.deepEqual(req.body.services.immich, { units: ['docker-immich', 'docker-immich-db'], timers: [] });
  assert.deepEqual(req.body.services.pihole.timers, ['gravity']);
});

test('apply: paints every dot (duplicates too) with state + tooltip/aria-label', () => {
  const a = fakeEl({ 'data-service-status': 'immich' });
  const b = fakeEl({ 'data-service-status': 'immich' });
  const c = fakeEl({ 'data-service-status': 'pihole' });
  const d = fakeEl({ 'data-service-status': 'gone' });
  dots.apply(fakeScope([a, b, c, d]), {
    services: {
      immich: { units: [{ name: 'docker-immich', active: 'failed', load: 'loaded', timer: false }] },
      pihole: {
        units: [
          { name: 'docker-pihole', active: 'active', load: 'loaded', timer: false },
          { name: 'gravity', active: 'inactive', load: 'loaded', timer: true },
        ],
      },
    },
  });
  assert.equal(a.attrs['data-state'], 'failed');
  assert.equal(b.attrs['data-state'], 'failed');
  assert.equal(a.attrs['aria-label'], 'immich: 1 failed');
  assert.equal(c.attrs['data-state'], 'ok');
  assert.equal(c.attrs.title, 'pihole: running');
  assert.equal(d.attrs['data-state'], 'none');
});

test('apply: server health and empty unit lists', () => {
  const a = fakeEl({ 'data-service-status': 'collabora' });
  const b = fakeEl({ 'data-service-status': 'ntp' });
  dots.apply(fakeScope([a, b]), {
    services: {
      collabora: {
        units: [
          { name: 'docker-collabora', active: 'active', health: 'running', load: 'loaded', timer: false },
          { name: 'collabora-setup', active: 'active', health: 'done', load: 'loaded', timer: false },
        ],
      },
      ntp: { units: [] },
    },
  });
  assert.equal(a.attrs['data-state'], 'ok');
  assert.equal(a.attrs.title, 'collabora: running');
  assert.equal(b.attrs['data-state'], 'idle');
  assert.equal(b.attrs.title, 'ntp: configured · no processes');
});

test('apply: failed request paints grey unknown', () => {
  const a = fakeEl({ 'data-service-status': 'immich' });
  dots.apply(fakeScope([a]), {});
  assert.equal(a.attrs['data-state'], 'none');
  assert.equal(a.attrs['aria-label'], 'immich: status unknown');
});

test('paintFresh: re-rendered dots take the last state when it covers their units', () => {
  const unpainted = (els) => ({
    querySelectorAll: () => els.filter((e) => e.getAttribute('data-state') === null),
  });
  const same = fakeEl({ 'data-service-status': 'immich', 'data-units': 'docker-immich-db docker-immich' });
  const grown = fakeEl({ 'data-service-status': 'pihole', 'data-units': 'docker-pihole gravity' });
  const added = fakeEl({ 'data-service-status': 'vikunja', 'data-units': 'vikunja' });
  const painted = fakeEl({ 'data-service-status': 'immich', 'data-units': 'docker-immich', 'data-state': 'ok' });
  const left = dots.paintFresh(unpainted([same, grown, added, painted]), {
    services: {
      immich: {
        units: [
          { name: 'docker-immich', active: 'active', load: 'loaded', timer: false },
          { name: 'docker-immich-db', active: 'failed', load: 'loaded', timer: false },
        ],
      },
      pihole: { units: [{ name: 'docker-pihole', active: 'active', load: 'loaded', timer: false }] },
    },
  });
  assert.equal(left, 2);
  assert.equal(same.attrs['data-state'], 'failed');
  assert.equal(grown.attrs['data-state'], undefined);
  assert.equal(added.attrs['data-state'], undefined);
  assert.equal(painted.attrs['data-state'], 'ok');
  assert.equal(dots.paintFresh(unpainted([grown]), null), 1);
});
