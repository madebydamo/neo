// Restore wizard of the backup store's portal (templates/backup_store/client.html.hbs,
// docs/design/backup-store-rescue.md §3). One question per step; the answers
// fill in the test commands and the final command.
//
// Data: the JSON in #neo-wizard-data (web/backup_store.rs `wizard_data`).
// Markup hooks under [data-neo-wiz]:
//   [data-neo-wiz-step="<id>"]        one section per step, shown when current
//   [data-neo-wiz-field="<key>"]       input/select/textarea/radio bound to state[key]
//   [data-neo-wiz-fill="<name>"]       text set from fills(state, data)[name]
//   [data-neo-wiz-show="<cond>"]       shown when cond holds ("reach=ssh", "where!=server",
//                                      "key=yes|login", several joined with " & ")
//   [data-neo-wiz-next] / [data-neo-wiz-back] / [data-neo-wiz-goto="<step>"]
//   [data-neo-wiz-steps]               the step list, rendered here
// The answers stay in sessionStorage per client, so a reload or going back keeps them.
(function (root) {
  'use strict';

  var NEO_FLAKE = 'github:madebydamo/neo';
  var NIX = "nix --extra-experimental-features 'nix-command flakes'";
  var DEFAULT_KEY = '~/.ssh/id_ed25519';
  var INSTALLER_KEY = '/root/admin_key';
  var INSTALLER_SSH_KEY = '/root/.ssh/id_ed25519';

  var TITLES = {
    where: 'Where you restore',
    reach: 'Way to the backup',
    login: 'Your SSH login',
    key: 'Your key',
    machine: 'New machine',
    rescue: 'Restore from here',
    result: 'Run it',
  };

  function initialState(data) {
    return {
      where: 'computer',
      // '' = follow the automatic check.
      reach: '',
      login: '',
      loginOk: false,
      keyText: '',
      keyPath: DEFAULT_KEY,
      host: '',
      disk: '',
      folder: (data.folders && data.folders[0]) || '.',
      step: 'where',
    };
  }

  /** "https" or "ssh": the operator's choice, else the automatic check. */
  function reach(state, auto) {
    if (state.reach) return state.reach;
    if (!auto || auto.server === false || auto.browser === false) return auto ? 'ssh' : 'https';
    return 'https';
  }

  /** The steps for these answers, in order. */
  function steps(state, auto) {
    var r = reach(state, auto);
    if (state.where === 'server') return ['where', 'reach', 'key', 'rescue'];
    var list = ['where', 'reach'];
    if (r === 'ssh') list.push('login');
    list.push('key', 'machine', 'result');
    return list;
  }

  /** Words that need no quotes stay bare; placeholders such as <new IP> too. */
  function shellArg(s) {
    s = String(s);
    // A placeholder (`<new IP>`, also inside `root@<new IP>`) stays as written: the reader replaces it.
    if (/^[A-Za-z0-9@%+=:,./_~-]*$/.test(s.replace(/<[^<>']*>/g, ''))) return s;
    return "'" + s.replace(/'/g, "'\\''") + "'";
  }

  function rootHost(host) {
    host = String(host || '').trim();
    if (!host) return '';
    return host.indexOf('@') === -1 ? 'root@' + host : host;
  }

  /** A login as ssh takes it: `user@host`, `ssh://…` or an alias; no spaces. */
  function loginOk(login) {
    return /^[^\s'"`;|&$<>]+$/.test(String(login || '').trim());
  }

  function hostOk(host) {
    return /^root@[^\s'"`;|&$<>@-][^\s'"`;|&$<>@]*$/.test(rootHost(host));
  }

  function folderPart(folder) {
    var f = String(folder || '').replace(/^\/+|\/+$/g, '');
    return f === '.' ? '' : f;
  }

  /** `--from-store …` with this client, or the explicit form for one of several folders. */
  function storeArgs(state, data) {
    var login = String(state.login || '').trim() || '<your login to this server>';
    if (data.folders && data.folders.length > 1) {
      return ['--from-backup', 'ssh+store://' + data.client + '@' + login + '/' + folderPart(state.folder)];
    }
    return ['--from-store', login, '--client', data.client];
  }

  function httpsArgs(state, data) {
    var url = (data.urls && data.urls[state.folder]) || data.url;
    return ['--from-backup', url];
  }

  /** The final command, as lines joined with " \\\n  ". */
  function command(state, auto, data) {
    var r = reach(state, auto);
    var zfs = data.format === 'zfs';
    var repo = data.gitRemote || '<your config repo>';
    var parts = [];
    var installer = state.where === 'nixos';
    parts.push((installer ? NIX : 'nix') + ' run ' + NEO_FLAKE + ' -- restore' + (zfs ? ' ' + shellArg(repo) : ''));
    var source = r === 'ssh' ? storeArgs(state, data) : zfs ? [] : httpsArgs(state, data);
    if (source.length) parts.push(source.map(shellArg).join(' '));
    if (installer) {
      var key = r === 'ssh' ? INSTALLER_SSH_KEY : INSTALLER_KEY;
      parts.push('--key ' + key + ' --shred-key');
    } else {
      parts.push('--host ' + shellArg(rootHost(state.host) || 'root@<new IP>') + ' --key ' + shellArg(state.keyPath || DEFAULT_KEY));
    }
    if (String(state.disk || '').trim()) parts.push('--disk ' + shellArg(state.disk.trim()));
    return parts.join(' \\\n  ');
  }

  /** Test of the SSH login, run where the restore runs. */
  function loginTest(state) {
    var login = String(state.login || '').trim() || '<your login>';
    return 'ssh ' + shellArg(login) + ' neo backup-store list';
  }

  /** Test of root SSH to the new machine, from your computer. */
  function machineTest(state) {
    var host = rootHost(state.host) || 'root@<new IP>';
    return 'ssh -i ' + shellArg(state.keyPath || DEFAULT_KEY) + ' ' + shellArg(host) + ' true && echo OK';
  }

  /** Copy the key to the installer, from your computer. */
  function keyCopy(state, auto) {
    var host = rootHost(state.host) || 'root@<installer IP>';
    var dest = reach(state, auto) === 'ssh' ? INSTALLER_SSH_KEY : INSTALLER_KEY;
    var mk = reach(state, auto) === 'ssh' ? "ssh " + shellArg(host) + " 'mkdir -p -m 700 /root/.ssh' && " : '';
    return mk + 'scp ' + shellArg(state.keyPath || DEFAULT_KEY) + ' ' + shellArg(host + ':' + dest);
  }

  // ── Keys ──────────────────────────────────────────────────────────────

  /**
   * What was pasted: a fingerprint (from `ssh-keygen -lf`), a public key
   * (its base64 blob), or a private key by mistake.
   */
  function parseKeyInput(text) {
    text = String(text || '').trim();
    if (!text) return { empty: true };
    if (/PRIVATE KEY/.test(text)) return { private: true };
    var fp = text.match(/SHA256:[A-Za-z0-9+/]{43}/);
    if (fp) return { fingerprint: fp[0] };
    var m = text.match(/(?:^|\s)((?:ssh|ecdsa|sk)-[A-Za-z0-9@.-]+)\s+([A-Za-z0-9+/]+={0,3})(?:\s|$)/);
    if (m) return { type: m[1], blob: m[2] };
    return { invalid: true };
  }

  function base64ToBytes(b64) {
    if (typeof atob === 'function') {
      var bin = atob(b64);
      var out = new Uint8Array(bin.length);
      for (var i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
      return out;
    }
    return new Uint8Array(Buffer.from(b64, 'base64'));
  }

  function bytesToBase64(bytes) {
    if (typeof btoa === 'function') {
      var s = '';
      for (var i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i]);
      return btoa(s);
    }
    return Buffer.from(bytes).toString('base64');
  }

  /** `SHA256:…` of a public key blob, as ssh-keygen prints it. */
  function fingerprintOf(blob, subtle) {
    return subtle.digest('SHA-256', base64ToBytes(blob)).then(function (d) {
      return 'SHA256:' + bytesToBase64(new Uint8Array(d)).replace(/=+$/, '');
    });
  }

  /**
   * What a fingerprint may do with this client's backup: `yes` opens it,
   * `login` logs in but cannot open it, `server` is the server's own key,
   * `no` is not accepted.
   */
  function keyVerdict(fingerprint, keys) {
    var hit = (keys || []).filter(function (k) {
      return k.fingerprint === fingerprint;
    })[0];
    if (!hit) return 'no';
    if (hit.role === 'server') return 'server';
    if (hit.role === 'access') return 'login';
    return 'yes';
  }

  /** The keys that can restore (not the server's own, not login-only). */
  function restoreKeys(keys) {
    return (keys || []).filter(function (k) {
      return k.role !== 'server' && k.role !== 'access';
    });
  }

  // ── Steps ─────────────────────────────────────────────────────────────

  /** Why Next is off on this step, or '' when it may go on. */
  function blocker(step, state, auto, data) {
    var r = reach(state, auto);
    switch (step) {
      case 'where':
        if (state.where === 'server' && !data.canRescue) return data.rescueOffReason || 'Not possible here.';
        return '';
      case 'reach':
        if (state.where === 'server' && r === 'ssh') {
          return 'This server restores over its web address only. Choose "From your computer" instead.';
        }
        return '';
      case 'login':
        if (!loginOk(state.login)) return 'Enter your login, for example ' + (data.loginExample || 'homeserver@server') + '.';
        if (!state.loginOk) return 'Run the test, then tick "It worked".';
        return '';
      case 'machine':
        if (state.where !== 'nixos' && !hostOk(state.host)) return 'Enter the new machine as root@IP.';
        return '';
      default:
        return '';
    }
  }

  function neighbour(state, auto, dir) {
    var list = steps(state, auto);
    var i = list.indexOf(state.step);
    if (i === -1) return list[0];
    var j = Math.max(0, Math.min(list.length - 1, i + dir));
    return list[j];
  }

  /** Every text the page fills in. */
  function fills(state, auto, data) {
    var r = reach(state, auto);
    return {
      command: command(state, auto, data),
      loginTest: loginTest(state),
      machineTest: machineTest(state),
      keyCopy: keyCopy(state, auto),
      login: String(state.login || '').trim() || '<your login>',
      host: rootHost(state.host) || 'root@<new IP>',
      keyPath: state.keyPath || DEFAULT_KEY,
      way: r === 'ssh' ? 'over SSH to this server' : 'over the web address',
    };
  }

  /** "a=b", "a!=b", "a=b|c", joined with " & ". */
  function holds(cond, view) {
    return String(cond)
      .split(/\s*&\s*/)
      .every(function (c) {
        var neg = c.indexOf('!=') !== -1;
        var kv = c.split(neg ? '!=' : '=');
        var v = String(view[kv[0]] == null ? '' : view[kv[0]]);
        var hit = kv.length < 2 ? v !== '' && v !== 'false' : kv[1].split('|').indexOf(v) !== -1;
        return neg ? !hit : hit;
      });
  }

  // ── DOM ───────────────────────────────────────────────────────────────

  function mount(el, doc, win) {
    var dataEl = doc.getElementById('neo-wizard-data');
    if (!dataEl) return;
    var data = JSON.parse(dataEl.textContent || '{}');
    var storeKey = 'neo-wizard-' + data.client;
    var state = initialState(data);
    try {
      var saved = JSON.parse(win.sessionStorage.getItem(storeKey) || 'null');
      if (saved && typeof saved === 'object') {
        Object.keys(state).forEach(function (k) {
          if (k in saved) state[k] = saved[k];
        });
      }
    } catch (e) {
      /* no storage: start fresh */
    }
    // Server: the tunnel unit. Browser: filled in by the probe below (null = checking).
    var auto = { server: !data.tunnelDown, browser: null };
    var keyFp = { text: null, verdict: '', fingerprint: '' };

    function save() {
      try {
        win.sessionStorage.setItem(storeKey, JSON.stringify(state));
      } catch (e) {
        /* fine */
      }
    }

    function view() {
      var r = reach(state, auto);
      return {
        where: state.where,
        reach: r,
        auto: auto.browser === null ? 'checking' : auto.server && auto.browser ? 'up' : 'down',
        server: auto.server ? 'up' : 'down',
        browser: auto.browser === null ? 'checking' : auto.browser ? 'up' : 'down',
        chosen: state.reach ? 'yes' : 'no',
        key: keyFp.verdict,
        loginOk: state.loginOk ? 'yes' : 'no',
        format: data.format,
        internet: data.internet ? 'yes' : 'no',
        canRescue: data.canRescue ? 'yes' : 'no',
        many: data.folders && data.folders.length > 1 ? 'yes' : 'no',
      };
    }

    function checkKey() {
      if (keyFp.text === state.keyText) return;
      keyFp.text = state.keyText;
      var p = parseKeyInput(state.keyText);
      var set = function (verdict, fp) {
        if (keyFp.text !== state.keyText) return;
        keyFp.verdict = verdict;
        keyFp.fingerprint = fp || '';
        render();
      };
      if (p.empty) return set('');
      if (p.private) return set('private');
      if (p.invalid) return set('invalid');
      if (p.fingerprint) return set(keyVerdict(p.fingerprint, data.keys), p.fingerprint);
      var subtle = win.crypto && win.crypto.subtle;
      if (!subtle) return set('nocrypto');
      fingerprintOf(p.blob, subtle).then(
        function (fp) {
          set(keyVerdict(fp, data.keys), fp);
        },
        function () {
          set('invalid');
        },
      );
    }

    function render() {
      var list = steps(state, auto);
      if (list.indexOf(state.step) === -1) state.step = list[list.length - 1];
      var v = view();
      el.querySelectorAll('[data-neo-wiz-step]').forEach(function (s) {
        s.hidden = s.getAttribute('data-neo-wiz-step') !== state.step;
      });
      el.querySelectorAll('[data-neo-wiz-show]').forEach(function (s) {
        s.hidden = !holds(s.getAttribute('data-neo-wiz-show'), v);
      });
      var f = fills(state, auto, data);
      f.fingerprint = keyFp.fingerprint;
      el.querySelectorAll('[data-neo-wiz-fill]').forEach(function (s) {
        var t = f[s.getAttribute('data-neo-wiz-fill')];
        if (t != null && s.textContent !== t) s.textContent = t;
      });
      el.querySelectorAll('[data-neo-wiz-field]').forEach(function (input) {
        var k = input.getAttribute('data-neo-wiz-field');
        if (input.type === 'radio') input.checked = String(state[k] || (k === 'reach' ? v.reach : '')) === input.value;
        else if (input.type === 'checkbox') input.checked = !!state[k];
        else if (doc.activeElement !== input && input.value !== String(state[k] == null ? '' : state[k])) {
          input.value = state[k] == null ? '' : state[k];
        }
      });
      el.querySelectorAll('[data-neo-wiz-disabled-when]').forEach(function (input) {
        input.disabled = holds(input.getAttribute('data-neo-wiz-disabled-when'), v);
      });
      var why = blocker(state.step, state, auto, data);
      el.querySelectorAll('[data-neo-wiz-step="' + state.step + '"] [data-neo-wiz-next]').forEach(function (b) {
        b.disabled = !!why;
      });
      el.querySelectorAll('[data-neo-wiz-step="' + state.step + '"] [data-neo-wiz-why]').forEach(function (b) {
        b.textContent = why;
        b.hidden = !why;
      });
      var nav = el.querySelector('[data-neo-wiz-steps]');
      if (nav) {
        var here = list.indexOf(state.step);
        nav.innerHTML = '';
        list.forEach(function (id, i) {
          var li = doc.createElement('li');
          li.className = 'step' + (i <= here ? ' step-primary' : '');
          li.textContent = TITLES[id] || id;
          if (i < here) {
            li.setAttribute('data-neo-wiz-goto', id);
            li.classList.add('cursor-pointer');
          }
          nav.appendChild(li);
        });
      }
      save();
    }

    el.addEventListener('input', function (e) {
      var k = e.target.getAttribute && e.target.getAttribute('data-neo-wiz-field');
      if (!k) return;
      if (e.target.type === 'radio' || e.target.type === 'checkbox') return;
      state[k] = e.target.value;
      // A new login needs a new test.
      if (k === 'login') state.loginOk = false;
      if (k === 'keyText') checkKey();
      render();
    });
    el.addEventListener('change', function (e) {
      var k = e.target.getAttribute && e.target.getAttribute('data-neo-wiz-field');
      if (!k) return;
      if (e.target.type === 'radio') state[k] = e.target.value;
      else if (e.target.type === 'checkbox') state[k] = e.target.checked;
      else state[k] = e.target.value;
      render();
    });
    el.addEventListener('click', function (e) {
      var t = e.target.closest ? e.target.closest('[data-neo-wiz-next],[data-neo-wiz-back],[data-neo-wiz-goto],[data-neo-wiz-set]') : null;
      if (!t) return;
      if (t.hasAttribute('data-neo-wiz-next')) {
        if (blocker(state.step, state, auto, data)) return;
        state.step = neighbour(state, auto, 1);
      } else if (t.hasAttribute('data-neo-wiz-back')) {
        state.step = neighbour(state, auto, -1);
      } else if (t.hasAttribute('data-neo-wiz-goto')) {
        state.step = t.getAttribute('data-neo-wiz-goto');
      } else {
        // data-neo-wiz-set="where=computer"
        var kv = t.getAttribute('data-neo-wiz-set').split('=');
        state[kv[0]] = kv[1];
      }
      render();
      var top = el.querySelector('[data-neo-wiz-steps]');
      if (top && top.scrollIntoView && t.hasAttribute('data-neo-wiz-next')) top.scrollIntoView({ block: 'nearest' });
    });

    // The rescue form's host field follows the machine answer.
    var hostInput = el.querySelector('form [name="host"]');
    if (hostInput && state.host && !hostInput.value) hostInput.value = rootHost(state.host);

    // Does this browser reach the store's web address? An opaque answer is enough.
    var probe = data.storeUrl && win.fetch ? data.storeUrl.replace(/\/*$/, '/') : '';
    if (probe && !data.tunnelDown) {
      var done = false;
      var timer = win.setTimeout(function () {
        if (!done) {
          done = true;
          auto.browser = false;
          render();
        }
      }, 8000);
      win.fetch(probe, { mode: 'no-cors', cache: 'no-store', credentials: 'omit' }).then(
        function () {
          if (done) return;
          done = true;
          win.clearTimeout(timer);
          auto.browser = true;
          render();
        },
        function () {
          if (done) return;
          done = true;
          win.clearTimeout(timer);
          auto.browser = false;
          render();
        },
      );
    } else {
      auto.browser = !data.tunnelDown;
    }
    checkKey();
    render();
  }

  var api = {
    initialState: initialState,
    reach: reach,
    steps: steps,
    shellArg: shellArg,
    rootHost: rootHost,
    loginOk: loginOk,
    hostOk: hostOk,
    command: command,
    loginTest: loginTest,
    machineTest: machineTest,
    keyCopy: keyCopy,
    parseKeyInput: parseKeyInput,
    fingerprintOf: fingerprintOf,
    keyVerdict: keyVerdict,
    restoreKeys: restoreKeys,
    blocker: blocker,
    holds: holds,
    fills: fills,
    mount: mount,
  };
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
  if (root && root.document) {
    root.NeoRestoreWizard = api;
    var start = function () {
      var el = root.document.querySelector('[data-neo-wiz]');
      if (el) mount(el, root.document, root);
    };
    if (root.document.readyState === 'loading') root.document.addEventListener('DOMContentLoaded', start);
    else start();
  }
})(typeof window !== 'undefined' ? window : null);
