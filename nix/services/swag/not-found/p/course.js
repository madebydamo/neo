/* Neo 404 — authored 8-stop course. Pure data + exit rules. No rendering. */
(function (root) {
  "use strict";

  var N = root.Neo404;
  if (!N) throw new Error("shared.js must load before course.js");

  function seg(partial) {
    return Object.assign(
      {
        id: "dns",
        title: "",
        place: "",
        length: 140,
        spliceAt: 24,
        spliceLen: 16,
        walls: [],
        hazards: [],
        pickups: [],
        signs: [],
        bounce: [],
        pits: [],
        mouths: [],
        gates: [],
        onEnter: "",
        theme: "void",
        correctLane: 1,
        onDie: {},
      },
      partial,
    );
  }

  function isPrivateHost(host) {
    var h = String(host || "").toLowerCase();
    return (
      /\.lab\b|\.home\b|\.lan\b|\.local\b|\.ts\.net\b|\.internal\b/.test(h) ||
      /^(ha|jellyfin|immich|photos|git|gitea|vault|home|hass|neo)(\.|$)/.test(h)
    );
  }

  function isTailnetHost(host) {
    var h = String(host || "").toLowerCase();
    return /\.ts\.net\b|tailscale|magicdns/.test(h);
  }

  function isPublicHost(host) {
    return !isPrivateHost(host) && !isTailnetHost(host);
  }

  function hash01(h, salt) {
    return ((N.hashStr(String(h) + ":" + salt) % 1000) / 1000);
  }

  function buildCourse(opts) {
    opts = opts || {};
    var path = opts.path || "/";
    var host = opts.host || "home.lab";
    var difficulty = N.DIFFICULTY[opts.difficulty] || N.DIFFICULTY.guest;
    var mapped = N.mapPath(path);
    var seed = N.hashStr(path + "|" + host);
    var guest = difficulty.id === "guest";
    var twoAm = difficulty.id === "2am";
    var sparse = difficulty.hazardScale < 0.8;
    var openIngress = 1;
    if (isPublicHost(host)) openIngress = 0;
    else if (isTailnetHost(host)) openIngress = 2;
    if (twoAm && hash01(seed, "shame") > 0.72) openIngress = 2;

    var proxyCorrect = 1;
    var proxyWrong = 0;
    var proxyHole = 2;
    if (mapped.container === "homepage") {
      proxyCorrect = 0;
      proxyWrong = 1;
      proxyHole = 2;
    } else if (hash01(seed, "proxy") > 0.5) {
      proxyWrong = 2;
      proxyHole = 0;
    }

    var dockerCorrect = 1;
    var dockerWrong = 0;
    var dockerDead = 2;
    if (hash01(seed, "dock") > 0.55) {
      dockerWrong = 2;
      dockerDead = 0;
    }

    var lying = twoAm && mapped.container === "jellyfin";
    var wanDnsSign = isPublicHost(host) ? "1.1.1.1  public" : "1.1.1.1  NXDOMAIN";
    var lanDnsSign = "Pi-hole  *.lab";
    var ovlDnsSign = isTailnetHost(host) ? "MagicDNS  tailnet" : "MagicDNS  maybe";
    var proxySigns = ["", "", ""];
    proxySigns[proxyCorrect] = mapped.sign;
    proxySigns[proxyWrong] = lying ? "Host(jel lyfin.lab)" : "Host(" + mapped.wrong + ".…)";
    proxySigns[proxyHole] = mapped.hole;

    var dockerNames = ["", "", ""];
    dockerNames[dockerCorrect] = mapped.container;
    dockerNames[dockerWrong] = mapped.wrong;
    dockerNames[dockerDead] = "unhealthy";

    var dnsCorrect = isTailnetHost(host) ? 2 : isPublicHost(host) ? 0 : 1;
    var routerCorrect = isPublicHost(host) ? 0 : 1;
    var tlsCorrect = openIngress;
    var authCorrect = 1;
    var originLane = dockerCorrect;

    function hz(lane, at, kind, name) {
      return { lane: lane, at: at, kind: kind, name: name || kind };
    }

    function gt(lane, from, to, spec) {
      return Object.assign(
        { lane: lane, from: from, to: to, name: "gate", label: "GATE" },
        spec || {},
      );
    }

    var stops = [];

    stops.push(
      seg({
        id: "socket",
        title: "Socket",
        place: "the browser",
        length: 42,
        spliceAt: -1,
        spliceLen: 0,
        theme: "socket",
        correctLane: 1,
        onEnter: "starting GET " + path,
        signs: [{ lane: 1, text: "GET " + path }],
      }),
    );

    stops.push(
      seg({
        id: "dns",
        title: "DNS",
        place: "Pi-hole / AdGuard / MagicDNS",
        length: 158,
        spliceAt: 26,
        spliceLen: 18,
        theme: "dns",
        correctLane: dnsCorrect,
        onEnter: "resolving " + host,
        signs: [
          { lane: 0, text: wanDnsSign },
          { lane: 1, text: lanDnsSign },
          { lane: 2, text: ovlDnsSign },
        ],
        hazards: sparse
          ? [hz(0, 88, "jump", "sinkhole")]
          : [hz(0, 78, "jump", "sinkhole"), hz(2, 104, "jump", "sinkhole"), hz(1, 128, "jump", "sinkhole")],
        pickups: [
          { lane: dnsCorrect, at: 96, id: "A" },
          { lane: 2, at: 70, id: "WG" },
        ],
        gates: [0, 1, 2].map(function (lane) {
          var refused = lane === 2 && isPublicHost(host);
          return gt(lane, 142, 157, {
            need: "A",
            name: refused ? "refused" : "nxdomain",
            label: refused ? "REFUSED" : "NXDOMAIN",
          });
        }),
        onDie: {
          sinkhole: N.FAIL_LINES.sinkhole,
          nxdomain: N.FAIL_LINES.nxdomain,
          refused: N.FAIL_LINES.refused,
        },
      }),
    );

    stops.push(
      seg({
        id: "router",
        title: "First hop",
        place: "the living-room router",
        length: 148,
        spliceAt: 24,
        spliceLen: 16,
        theme: "router",
        correctLane: routerCorrect,
        onEnter: "first hop · NAT",
        signs: [
          { lane: 0, text: "WAN ACL" },
          { lane: 1, text: "hairpin NAT" },
          { lane: 2, text: "wg0" },
        ],
        hazards: sparse
          ? [hz(1, 72, "jump", "cat"), hz(0, 112, "duck", "banhammer")]
          : [
              hz(1, 68, "jump", "cat"),
              hz(0, 86, "jump", "rst"),
              hz(2, 86, "jump", "rst"),
              hz(0, 118, "duck", "banhammer"),
              hz(1, 118, "duck", "banhammer"),
              hz(2, 118, "duck", "banhammer"),
            ],
        pickups: [{ lane: 1, at: 98, id: "SRC" }],
        gates: (function () {
          var g = [
            gt(2, 118, 146, { need: "WG", name: "wanAcl", label: "NO WG" }),
          ];
          if (!isPublicHost(host)) {
            g.push(gt(0, 118, 146, { need: "WG", name: "wanAcl", label: "WAN ACL" }));
          }
          return g;
        })(),
        onDie: {
          cat: "the rack cat collected a packet",
          rst: N.FAIL_LINES.rst,
          banhammer: "port-scan banhammer. you ducked too late",
          wanAcl: N.FAIL_LINES.wanAcl,
        },
      }),
    );

    var ingressWalls = [];
    [0, 1, 2].forEach(function (lane) {
      if (lane !== openIngress) {
        ingressWalls.push({
          lane: lane,
          from: 70,
          to: 148,
          name: lane === 2 ? "closedPort" : "timeout",
        });
      }
    });

    stops.push(
      seg({
        id: "ingress",
        title: "Ingress",
        place: "how the house accepts work",
        length: 148,
        spliceAt: 22,
        spliceLen: 18,
        theme: "ingress",
        correctLane: openIngress,
        onEnter: "choose a duct",
        signs: [
          { lane: 0, text: ":443  tunnel" },
          { lane: 1, text: ":443  proxy" },
          { lane: 2, text: ":8096  shame" },
        ],
        walls: ingressWalls,
        pickups: [{ lane: openIngress, at: 92, id: "SNI" }],
        hazards: sparse ? [] : [hz(openIngress, 118, "jump", "rst")],
        onDie: {
          closedPort: N.FAIL_LINES.closedPort,
          timeout: N.FAIL_LINES.timeout,
          rst: N.FAIL_LINES.rst,
        },
      }),
    );

    stops.push(
      seg({
        id: "tls",
        title: "TLS",
        place: "certificate, SNI, HTTP vs HTTPS",
        length: 152,
        spliceAt: 24,
        spliceLen: 16,
        theme: "tls",
        correctLane: tlsCorrect,
        onEnter: "handshake",
        signs: [
          { lane: 0, text: "SNI / 443" },
          { lane: 1, text: "lab CA" },
          { lane: 2, text: "plaintext sin" },
        ],
        hazards: sparse
          ? [hz(tlsCorrect, 82, "jump", "expired")]
          : [
              hz(0, 76, "jump", "expired"),
              hz(1, 76, "jump", "expired"),
              hz(2, 96, "jump", "expired"),
              hz(0, 114, "duck", "hsts"),
              hz(1, 114, "duck", "hsts"),
            ],
        pickups: [{ lane: tlsCorrect, at: 100, id: "PADLOCK" }],
        gates: [0, 1].map(function (lane) {
          return gt(lane, 128, 150, {
            needAny: ["SNI", "PADLOCK"],
            name: "tls",
            label: "NO SNI",
          });
        }),
        onDie: {
          expired: N.FAIL_LINES.tls,
          hsts: "HSTS slapped you into next week",
          tls: N.FAIL_LINES.tls,
        },
      }),
    );

    stops.push(
      seg({
        id: "proxy",
        title: "Reverse proxy",
        place: "Traefik / Caddy / nginx",
        length: 176,
        spliceAt: 28,
        spliceLen: 20,
        theme: "proxy",
        correctLane: proxyCorrect,
        onEnter: "routers incoming",
        signs: [
          { lane: 0, text: proxySigns[0] },
          { lane: 1, text: proxySigns[1] },
          { lane: 2, text: proxySigns[2] },
        ],
        pits: [{ lane: proxyHole, from: 88, to: 176, name: "noRouter" }],
        bounce: sparse ? [] : [{ lane: proxyCorrect, at: 70, name: "https-redirect" }],
        hazards: sparse
          ? [hz(proxyCorrect, 124, "duck", "ratelimit")]
          : [
              hz(proxyCorrect, 118, "duck", "ratelimit"),
              hz(proxyWrong, 118, "duck", "ratelimit"),
            ],
        pickups: [
          { lane: proxyCorrect, at: 96, id: "HOST" },
          { lane: proxyCorrect, at: 140, id: "XFF" },
        ],
        onDie: {
          noRouter: N.FAIL_LINES.noRouter,
          ratelimit: "429. the boom arm got you",
        },
      }),
    );

    stops.push(
      seg({
        id: "auth",
        title: "Auth gate",
        place: "Authelia / Authentik / I left it open",
        length: 136,
        spliceAt: 22,
        spliceLen: 16,
        theme: "auth",
        correctLane: authCorrect,
        onEnter: "turnstile",
        signs: [
          { lane: 0, text: "401  login" },
          { lane: 1, text: "bypass internal" },
          { lane: 2, text: "cookie?" },
        ],
        walls: [{ lane: 0, from: 78, to: 136, name: "auth401" }],
        hazards: sparse ? [] : [hz(2, 96, "duck", "ratelimit")],
        pickups: [{ lane: 2, at: 64, id: "COOKIE" }],
        onDie: {
          auth401: N.FAIL_LINES.auth401,
          ratelimit: "429. the boom arm got you",
        },
      }),
    );

    if (twoAm) {
      stops[stops.length - 1].hazards.push(hz(1, 110, "jump", "oom"));
    }

    stops.push(
      seg({
        id: "docker",
        title: "Docker network",
        place: "bridge / host / the overlay you spelled wrong",
        length: 168,
        spliceAt: 26,
        spliceLen: 18,
        theme: "docker",
        correctLane: dockerCorrect,
        onEnter: "container mouths",
        signs: [
          { lane: 0, text: dockerNames[0] },
          { lane: 1, text: dockerNames[1] },
          { lane: 2, text: dockerNames[2] },
        ],
        mouths: [
          { lane: dockerCorrect, from: 128, to: 168, kind: "correct", name: mapped.container },
          { lane: dockerWrong, from: 128, to: 168, kind: "wrong", name: mapped.wrong },
          { lane: dockerDead, from: 128, to: 168, kind: "dead", name: "unhealthy" },
        ],
        hazards: sparse
          ? [hz(dockerDead, 72, "jump", "oom"), hz(dockerCorrect, 100, "jump", "restarting")]
          : [
              hz(dockerDead, 70, "jump", "oom"),
              hz(dockerWrong, 88, "jump", "unhealthy"),
              hz(dockerCorrect, 104, "jump", "restarting"),
            ],
        pickups: [
          { lane: dockerCorrect, at: 118, id: "CID" },
          { lane: dockerCorrect, at: 84, id: "RANGE" },
        ],
        onDie: {
          oom: N.FAIL_LINES.oom,
          unhealthy: N.FAIL_LINES.badGateway,
          restarting: N.FAIL_LINES.badGateway,
          badGateway: N.FAIL_LINES.badGateway,
        },
      }),
    );

    stops.push(
      seg({
        id: "origin",
        title: "The app",
        place: "inside the process",
        length: 88,
        spliceAt: 18,
        spliceLen: 14,
        theme: "origin",
        correctLane: originLane,
        onEnter: "infrastructure: 200",
        signs: [
          { lane: originLane, text: path },
          { lane: (originLane + 1) % 3, text: "/  Dashboard" },
        ],
        onDie: { origin404: N.FAIL_LINES.origin404 },
      }),
    );

    var z = 0;
    stops.forEach(function (s) {
      s.worldZ = z;
      s.endZ = z + s.length;
      z += s.length;
    });

    return {
      path: path,
      host: host,
      difficulty: difficulty.id,
      mapped: mapped,
      totalLength: z,
      stops: stops,
      openIngress: openIngress,
      dnsCorrect: dnsCorrect,
      proxyCorrect: proxyCorrect,
      proxyWrong: proxyWrong,
      dockerCorrect: dockerCorrect,
      dockerWrong: dockerWrong,
      dockerDead: dockerDead,
    };
  }

  function stopAt(course, z) {
    var i;
    var s;
    for (i = 0; i < course.stops.length; i++) {
      s = course.stops[i];
      if (z < s.endZ) return s;
    }
    return course.stops[course.stops.length - 1];
  }

  function localZ(stop, z) {
    return z - stop.worldZ;
  }

  function inSplice(stop, z) {
    if (!stop || stop.spliceAt < 0) return false;
    var lz = localZ(stop, z);
    return lz >= stop.spliceAt && lz <= stop.spliceAt + stop.spliceLen;
  }

  function hasTag(tags, id) {
    if (!tags) return false;
    if (typeof tags.has === "function") return tags.has(id);
    return tags.indexOf(id) !== -1;
  }

  function gateOpen(gate, tags) {
    var i;
    if (!gate) return true;
    if (gate.needAny && gate.needAny.length) {
      for (i = 0; i < gate.needAny.length; i++) {
        if (hasTag(tags, gate.needAny[i])) return true;
      }
      return false;
    }
    if (gate.need) return hasTag(tags, gate.need);
    return false;
  }

  function die(key, line, stop) {
    return {
      ok: false,
      ending: N.ENDINGS.transit,
      key: key,
      line: (stop && stop.onDie && stop.onDie[key]) || line || N.FAIL_LINES[key] || key,
    };
  }

  function ok(extra) {
    return Object.assign({ ok: true }, extra || {});
  }

  function evaluateExit(stop, packet, ctx) {
    ctx = ctx || {};
    var lane = packet.lane;
    var tags = packet.tags;
    var host = ctx.host || "";
    if (!stop) return ok();

    if (stop.id === "dns") {
      if (!hasTag(tags, "A")) {
        if (lane === 2 && isPublicHost(host)) return die("refused", N.FAIL_LINES.refused, stop);
        if (lane === 0 && isPrivateHost(host)) return die("nxdomain", N.FAIL_LINES.nxdomain, stop);
        if (lane === 2 && !isTailnetHost(host)) return die("nxdomain", N.FAIL_LINES.nxdomain, stop);
        return die("nxdomain", N.FAIL_LINES.nxdomain, stop);
      }
      return ok();
    }

    if (stop.id === "router") {
      if (lane === 0 && !hasTag(tags, "WG") && !isPublicHost(host)) return die("wanAcl", N.FAIL_LINES.wanAcl, stop);
      if (lane === 2 && !hasTag(tags, "WG")) return die("wanAcl", "no session on wg0. handshake never happened", stop);
      return ok();
    }

    if (stop.id === "ingress") {
      return ok();
    }

    if (stop.id === "tls") {
      if (lane !== 2 && !hasTag(tags, "SNI") && !hasTag(tags, "PADLOCK")) {
        return die("tls", N.FAIL_LINES.tls, stop);
      }
      return ok();
    }

    if (stop.id === "proxy") {
      if (lane === ctx.proxyHole || (ctx.course && lane === ctx.course.proxyHole)) {
        return die("noRouter", N.FAIL_LINES.noRouter, stop);
      }
      if (lane === ctx.proxyWrong || (ctx.course && lane === ctx.course.proxyWrong)) {
        return ok({ tag: "WRONG_APP" });
      }
      return ok();
    }

    if (stop.id === "auth") {
      if (lane === 0 && !hasTag(tags, "COOKIE")) return die("auth401", N.FAIL_LINES.auth401, stop);
      return ok();
    }

    if (stop.id === "docker") {
      var mouths = stop.mouths || [];
      var i;
      var m;
      for (i = 0; i < mouths.length; i++) {
        m = mouths[i];
        if (m.lane === lane) {
          if (m.kind === "dead") return die("badGateway", N.FAIL_LINES.badGateway, stop);
          if (m.kind === "wrong") return ok({ tag: "WRONG_APP", endingHint: N.ENDINGS.wrongApp });
          return ok();
        }
      }
      return die("badGateway", N.FAIL_LINES.badGateway, stop);
    }

    if (stop.id === "origin") {
      if (hasTag(tags, "WRONG_APP")) {
        return {
          ok: true,
          ending: N.ENDINGS.wrongApp,
          line: "200 OK — wrong living room. this is not " + (ctx.path || "that path"),
        };
      }
      return {
        ok: true,
        ending: N.ENDINGS.origin404,
        line: "infrastructure: 200\napplication: 404\nthe box is fine. this path is not.",
      };
    }

    return ok();
  }

  function hitBox(kind, ducking, y) {
    if (kind === "duck") return !ducking;
    if (kind === "jump") return y < 0.72;
    return true;
  }

  var api = {
    buildCourse: buildCourse,
    stopAt: stopAt,
    localZ: localZ,
    inSplice: inSplice,
    evaluateExit: evaluateExit,
    hitBox: hitBox,
    hasTag: hasTag,
    gateOpen: gateOpen,
    isPrivateHost: isPrivateHost,
    isTailnetHost: isTailnetHost,
    isPublicHost: isPublicHost,
  };

  root.Neo404 = Object.assign(root.Neo404 || {}, api);
  if (typeof module !== "undefined" && module.exports) module.exports = root.Neo404;
})(typeof globalThis !== "undefined" ? globalThis : this);
