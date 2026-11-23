'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const w = require('./backup_store_wizard.js');

const data = {
  client: 'meilen',
  format: 'encrypted',
  url: 'https://meilen@backup.example.com/backup/meilen',
  urls: {
    'backup/meilen': 'https://meilen@backup.example.com/backup/meilen',
    old: 'https://meilen@backup.example.com/old',
  },
  storeUrl: 'https://backup.example.com',
  folders: ['backup/meilen'],
  keys: [
    { label: 'root@meilen', fingerprint: 'SHA256:srv', role: 'server' },
    { label: 'me', fingerprint: 'SHA256:me', role: 'unlock' },
    { label: 'old', fingerprint: 'SHA256:old', role: 'access' },
  ],
  canRescue: true,
  loginExample: 'homeserver@brugg',
};

const up = { server: true, browser: true };
const down = { server: true, browser: false };

function state(over) {
  return Object.assign(w.initialState(data), over || {});
}

test('from your computer is the default, and the check picks the way', () => {
  const s = state();
  assert.equal(s.where, 'computer');
  assert.equal(w.reach(s, up), 'https');
  assert.equal(w.reach(s, down), 'ssh');
  assert.equal(w.reach(s, { server: false, browser: null }), 'ssh');
  // Still checking: the web address until it fails.
  assert.equal(w.reach(s, { server: true, browser: null }), 'https');
  // The operator's choice wins.
  assert.equal(w.reach(state({ reach: 'ssh' }), up), 'ssh');
  assert.equal(w.reach(state({ reach: 'https' }), down), 'https');
});

test('steps follow the answers', () => {
  assert.deepEqual(w.steps(state(), up), ['where', 'reach', 'key', 'machine', 'result']);
  assert.deepEqual(w.steps(state(), down), ['where', 'reach', 'login', 'key', 'machine', 'result']);
  assert.deepEqual(w.steps(state({ where: 'server' }), up), ['where', 'reach', 'key', 'rescue']);
  assert.deepEqual(w.steps(state({ where: 'nixos', reach: 'ssh' }), up), [
    'where',
    'reach',
    'login',
    'key',
    'machine',
    'result',
  ]);
});

test('commands carry the answers', () => {
  const https = w.command(state({ host: '203.0.113.7' }), up, data);
  assert.equal(
    https,
    'nix run github:madebydamo/neo -- restore \\\n  --from-backup https://meilen@backup.example.com/backup/meilen \\\n  --host root@203.0.113.7 --key ~/.ssh/id_ed25519',
  );
  const ssh = w.command(state({ host: 'root@x', login: 'homeserver@brugg', disk: '/dev/sdb', keyPath: '~/.ssh/admin key' }), down, data);
  assert.match(ssh, /--from-store homeserver@brugg --client meilen/);
  assert.match(ssh, /--key '~\/.ssh\/admin key'/);
  assert.match(ssh, /--disk \/dev\/sdb$/);
  // Several folders: the explicit form names the folder.
  const many = Object.assign({}, data, { folders: ['backup/meilen', 'old'] });
  assert.match(w.command(state({ login: 'b', folder: 'old' }), down, many), /--from-backup ssh\+store:\/\/meilen@b\/old/);
  assert.match(w.command(state({ folder: 'old' }), up, many), /--from-backup https:\/\/meilen@backup.example.com\/old/);
  // The installer: its own key path, no --host.
  const nixos = w.command(state({ where: 'nixos' }), up, data);
  assert.match(nixos, /^nix --extra-experimental-features 'nix-command flakes' run/);
  assert.match(nixos, /--key \/root\/admin_key --shred-key/);
  assert.doesNotMatch(nixos, /--host/);
  assert.match(w.command(state({ where: 'nixos', login: 'h@b' }), down, data), /--key \/root\/.ssh\/id_ed25519 --shred-key/);
  // ZFS: the repo, no --from-backup.
  const zfs = Object.assign({}, data, { format: 'zfs', gitRemote: 'git@github.com:me/c.git', folders: [] });
  const z = w.command(state({ host: 'root@x' }), up, zfs);
  assert.match(z, /restore git@github.com:me\/c.git/);
  assert.doesNotMatch(z, /--from-backup/);
  // Empty answers stay placeholders.
  assert.match(w.command(state(), down, data), /--from-store <your login to this server>.*--host root@<new IP>/s);
});

test('test commands', () => {
  assert.equal(w.loginTest(state({ login: 'homeserver@brugg' })), 'ssh homeserver@brugg neo backup-store list');
  assert.equal(w.machineTest(state({ host: '1.2.3.4' })), 'ssh -i ~/.ssh/id_ed25519 root@1.2.3.4 true && echo OK');
  assert.equal(w.keyCopy(state({ host: '10.0.0.5' }), up), 'scp ~/.ssh/id_ed25519 root@10.0.0.5:/root/admin_key');
  assert.match(w.keyCopy(state({ host: '10.0.0.5' }), down), /mkdir -p -m 700 \/root\/.ssh.*:\/root\/.ssh\/id_ed25519$/);
});

test('next stays off until the step is answered', () => {
  assert.equal(w.blocker('where', state(), up, data), '');
  assert.match(w.blocker('where', state({ where: 'server' }), up, Object.assign({}, data, { canRescue: false, rescueOffReason: 'login off' })), /login off/);
  assert.match(w.blocker('reach', state({ where: 'server', reach: 'ssh' }), up, data), /From your computer/);
  assert.match(w.blocker('login', state({ login: '' }), down, data), /homeserver@brugg/);
  assert.match(w.blocker('login', state({ login: 'a b' }), down, data), /Enter your login/);
  assert.match(w.blocker('login', state({ login: 'h@b' }), down, data), /It worked/);
  assert.equal(w.blocker('login', state({ login: 'h@b', loginOk: true }), down, data), '');
  assert.match(w.blocker('machine', state({ host: '' }), up, data), /root@IP/);
  assert.match(w.blocker('machine', state({ host: 'root@-oProxyCommand=x' }), up, data), /root@IP/);
  assert.equal(w.blocker('machine', state({ host: '203.0.113.7' }), up, data), '');
  assert.equal(w.blocker('machine', state({ where: 'nixos' }), up, data), '');
});

test('pasted keys are read and judged by their role', async () => {
  assert.deepEqual(w.parseKeyInput(''), { empty: true });
  assert.deepEqual(w.parseKeyInput('-----BEGIN OPENSSH PRIVATE KEY-----\nabc'), { private: true });
  assert.deepEqual(w.parseKeyInput('nonsense'), { invalid: true });
  const fp = 'SHA256:' + 'a'.repeat(43);
  assert.deepEqual(w.parseKeyInput('256 ' + fp + ' me@x (ED25519)'), { fingerprint: fp });
  // A real key: the fingerprint matches ssh-keygen's.
  const blob = 'AAAAC3NzaC1lZDI1NTE5AAAAIC9fFR8aERyyLjqI0aG08BXmSaMemGXK4WK8bLy7Nzmc';
  const p = w.parseKeyInput('ssh-ed25519 ' + blob + ' development');
  assert.equal(p.blob, blob);
  const got = await w.fingerprintOf(p.blob, crypto.webcrypto.subtle);
  assert.equal(got, 'SHA256:I5xbTMCVPVdlTtxs3tfmOi7TBZESFgWE870N2wNKyLc');

  assert.equal(w.keyVerdict('SHA256:me', data.keys), 'yes');
  assert.equal(w.keyVerdict('SHA256:old', data.keys), 'login');
  assert.equal(w.keyVerdict('SHA256:srv', data.keys), 'server');
  assert.equal(w.keyVerdict('SHA256:x', data.keys), 'no');
  // Roles unknown (files, zfs): any listed key.
  assert.equal(w.keyVerdict('SHA256:k', [{ fingerprint: 'SHA256:k', role: '' }]), 'yes');
  assert.deepEqual(
    w.restoreKeys(data.keys).map((k) => k.label),
    ['me'],
  );
});

test('conditions', () => {
  const v = { where: 'computer', reach: 'ssh', key: '' };
  assert.equal(w.holds('reach=ssh', v), true);
  assert.equal(w.holds('where!=server', v), true);
  assert.equal(w.holds('where=server & reach=ssh', v), false);
  assert.equal(w.holds('where=computer|nixos & reach=ssh', v), true);
  assert.equal(w.holds('key=yes', v), false);
});

test('quoting', () => {
  assert.equal(w.shellArg('root@1.2.3.4'), 'root@1.2.3.4');
  assert.equal(w.shellArg('<new IP>'), '<new IP>');
  assert.equal(w.shellArg("it's"), "'it'\\''s'");
  assert.equal(w.rootHost('1.2.3.4'), 'root@1.2.3.4');
  assert.equal(w.hostOk('root@[2001:db8::1]:2222'), true);
  assert.equal(w.loginOk('ssh://homeserver@brugg:2222'), true);
  assert.equal(w.loginOk('a;b'), false);
});
