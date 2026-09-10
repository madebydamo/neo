/* Neo 404 — shared contract. Loaded first in browser and node --test.
 *
 * You are one HTTP request running Subway-Surfers-style down a 3-lane cable.
 * Camera sits behind the packet, world streams toward the vanishing point.
 * Lanes (left→right): WAN=0, LAN=1, OVL=2.
 */
(function (root) {
  "use strict";

  var LANES = [
    {
      id: 0,
      key: "WAN",
      title: "WAN",
      hint: "internet / tunnel / port-forward",
      color: "#ff6a3d",
      glow: "rgba(255,106,61,0.85)",
    },
    {
      id: 1,
      key: "LAN",
      title: "LAN",
      hint: "hairpin on the home network",
      color: "#3df0c2",
      glow: "rgba(61,240,194,0.85)",
    },
    {
      id: 2,
      key: "OVL",
      title: "OVL",
      hint: "Tailscale / WG / Docker network",
      color: "#b388ff",
      glow: "rgba(179,136,255,0.9)",
    },
  ];

  var STOP_IDS = [
    "socket",
    "dns",
    "router",
    "ingress",
    "tls",
    "proxy",
    "auth",
    "docker",
    "origin",
  ];

  var DIFFICULTY = {
    guest: { id: "guest", speed: 14, speedPerStop: 1.15, hazardScale: 0.55, pulseMs: 400 },
    homelab: { id: "homelab", speed: 16, speedPerStop: 1.45, hazardScale: 1, pulseMs: 0 },
    "2am": { id: "2am", speed: 19, speedPerStop: 1.8, hazardScale: 1.35, pulseMs: 0, lyingSign: true },
  };

  var PHYS = {
    laneX: [-1.62, 0, 1.62],
    laneChangeMs: 120,
    graceS: 1.5,
    spliceS: 1.0,
    jumpV: 9.2,
    gravity: 32,
    standH: 0.92,
    duckH: 0.4,
    hitW: 0.62,
    camBack: 5.4,
    camHeight: 1.92,
    camLookAhead: 9.5,
    camFollowX: 0.52,
  };

  var PICKUP_META = {
    A: { label: "A", chip: "A record" },
    SNI: { label: "SNI", chip: "SNI" },
    PADLOCK: { label: "padlock", chip: "padlock" },
    SRC: { label: "src", chip: "src=10.0.0.x" },
    WG: { label: "WG", chip: "WG handshake" },
    XFF: { label: "XFF", chip: "X-Forwarded-For" },
    HOST: { label: "Host", chip: "Host rewrite" },
    COOKIE: { label: "Cookie", chip: "Cookie" },
    CID: { label: "cid", chip: "container_id" },
    RANGE: { label: "Range", chip: "Range" },
  };

  var ENDINGS = {
    origin404: "origin404",
    wrongApp: "wrongApp",
    transit: "transit",
  };

  var FAIL_LINES = {
    sinkhole: "query swallowed. this is not an ad, but gravity is tired",
    nxdomain: "no such host. split brain DNS, classic",
    refused: "REFUSED. the resolver folded its arms",
    wanAcl: "the living room declined your packet",
    closedPort: ":8096 is not the personality trait you think it is",
    timeout: "connection timed out. the duct was a painting",
    tls: "handshake failed. the lab CA is a family member",
    noRouter: "404 in the proxy. nobody booked this path",
    auth401: "authelia: no. you are a guest in your own house",
    badGateway: "backend up, then down, then up. you arrived on down",
    oom: "killed. rss was showing off",
    rst: "RST. the socket slammed",
    insulation: "dropped on the floor. insulation is not a lane",
    origin404: "you delivered a question the app cannot answer",
  };

  var PATH_MAP = [
    { re: /jellyfin|media|watch/, sign: "Host(jellyfin.…)", container: "jellyfin", wrong: "vaultwarden", hole: "PathPrefix(/api)" },
    { re: /immich|photos/, sign: "Host(photos.…)", container: "immich-server", wrong: "jellyfin", hole: "PathPrefix(/api)" },
    { re: /hass|homeassistant|\bha\b/, sign: "Host(ha.…)", container: "homeassistant", wrong: "immich-server", hole: "PathPrefix(/api)" },
    { re: /\bgit|\bgitea\b/, sign: "Host(git.…)", container: "gitea", wrong: "vaultwarden", hole: "Host(jellyfin.…)" },
    { re: /vault|pass|bitwarden/, sign: "Host(vault.…)", container: "vaultwarden", wrong: "gitea", hole: "PathPrefix(/api)" },
    { re: null, sign: "Host(home.…)", container: "homepage", wrong: "jellyfin", hole: "PathPrefix(/api)" },
  ];

  function hashStr(s) {
    var h = 2166136261;
    var i;
    s = String(s || "");
    for (i = 0; i < s.length; i++) {
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    return h >>> 0;
  }

  function mapPath(pathname) {
    var p = String(pathname || "/").toLowerCase();
    var i;
    for (i = 0; i < PATH_MAP.length; i++) {
      if (PATH_MAP[i].re && PATH_MAP[i].re.test(p)) return PATH_MAP[i];
    }
    return PATH_MAP[PATH_MAP.length - 1];
  }

  function lerp(a, b, t) {
    return a + (b - a) * t;
  }

  function clamp(v, lo, hi) {
    return Math.max(lo, Math.min(hi, v));
  }

  function nowMs() {
    return typeof performance !== "undefined" && performance.now ? performance.now() : Date.now();
  }

  var api = {
    LANES: LANES,
    STOP_IDS: STOP_IDS,
    DIFFICULTY: DIFFICULTY,
    PHYS: PHYS,
    PICKUP_META: PICKUP_META,
    ENDINGS: ENDINGS,
    FAIL_LINES: FAIL_LINES,
    PATH_MAP: PATH_MAP,
    hashStr: hashStr,
    mapPath: mapPath,
    lerp: lerp,
    clamp: clamp,
    nowMs: nowMs,
  };

  root.Neo404 = Object.assign(root.Neo404 || {}, api);
  if (typeof module !== "undefined" && module.exports) module.exports = root.Neo404;
})(typeof globalThis !== "undefined" ? globalThis : this);
