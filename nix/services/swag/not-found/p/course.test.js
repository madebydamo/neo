"use strict";

var test = require("node:test");
var assert = require("node:assert/strict");
require("./shared.js");
require("./course.js");
var N = globalThis.Neo404;

test("mapPath jellyfin", function () {
  assert.equal(N.mapPath("/jellyfin/web").container, "jellyfin");
  assert.equal(N.mapPath("/photos/api").container, "immich-server");
  assert.equal(N.mapPath("/unknown/xyz").container, "homepage");
});

test("guest course has 9 stops and LAN DNS default", function () {
  var c = N.buildCourse({ path: "/photos/foo", host: "photos.lab", difficulty: "guest" });
  assert.equal(c.stops.length, 9);
  assert.equal(c.stops[0].id, "socket");
  assert.equal(c.stops[8].id, "origin");
  assert.equal(c.dnsCorrect, 1);
  assert.equal(c.mapped.container, "immich-server");
  assert.ok(c.totalLength > 1000);
  c.stops.forEach(function (s) {
    if (s.id === "socket") return;
    var hazards = s.hazards || [];
    hazards.forEach(function (h) {
      assert.ok(h.at >= 14 * 1.4, s.id + " hazard too early: " + h.at);
    });
  });
});

test("dns NXDOMAIN on WAN without A for lab host", function () {
  var c = N.buildCourse({ path: "/", host: "jellyfin.lab", difficulty: "homelab" });
  var dns = c.stops.find(function (s) {
    return s.id === "dns";
  });
  var r = N.evaluateExit(dns, { lane: 0, tags: [] }, { host: "jellyfin.lab", course: c });
  assert.equal(r.ok, false);
  assert.match(r.line, /no such host/);
});

test("dns survives LAN with A record", function () {
  var c = N.buildCourse({ path: "/", host: "home.lab", difficulty: "guest" });
  var dns = c.stops.find(function (s) {
    return s.id === "dns";
  });
  var r = N.evaluateExit(dns, { lane: 1, tags: ["A"] }, { host: "home.lab", course: c });
  assert.equal(r.ok, true);
});

test("dns dies on LAN without A record", function () {
  var c = N.buildCourse({ path: "/", host: "home.lab", difficulty: "guest" });
  var dns = c.stops.find(function (s) {
    return s.id === "dns";
  });
  var r = N.evaluateExit(dns, { lane: 1, tags: [] }, { host: "home.lab", course: c });
  assert.equal(r.ok, false);
});

test("tls dies without SNI on LAN", function () {
  var c = N.buildCourse({ path: "/", host: "home.lab", difficulty: "homelab" });
  var tls = c.stops.find(function (s) {
    return s.id === "tls";
  });
  var r = N.evaluateExit(tls, { lane: 1, tags: ["A"] }, { host: "home.lab", course: c });
  assert.equal(r.ok, false);
  assert.match(r.line, /handshake/);
});

test("tls lives on OVL without SNI", function () {
  var c = N.buildCourse({ path: "/", host: "home.lab", difficulty: "homelab" });
  var tls = c.stops.find(function (s) {
    return s.id === "tls";
  });
  var r = N.evaluateExit(tls, { lane: 2, tags: ["A"] }, { host: "home.lab", course: c });
  assert.equal(r.ok, true);
});

test("proxy hole is mid-run 404", function () {
  var c = N.buildCourse({ path: "/jellyfin/x", host: "jellyfin.lab", difficulty: "homelab" });
  var proxy = c.stops.find(function (s) {
    return s.id === "proxy";
  });
  var r = N.evaluateExit(proxy, { lane: c.proxyHole, tags: ["A", "SNI"] }, { host: "jellyfin.lab", course: c });
  assert.equal(r.ok, false);
  assert.match(r.line, /nobody booked/);
});

test("wrong proxy tags WRONG_APP", function () {
  var c = N.buildCourse({ path: "/jellyfin/x", host: "jellyfin.lab", difficulty: "homelab" });
  var proxy = c.stops.find(function (s) {
    return s.id === "proxy";
  });
  var r = N.evaluateExit(proxy, { lane: c.proxyWrong, tags: ["A", "SNI"] }, { host: "jellyfin.lab", course: c });
  assert.equal(r.ok, true);
  assert.equal(r.tag, "WRONG_APP");
});

test("auth WAN without cookie is 401", function () {
  var c = N.buildCourse({ path: "/", host: "home.lab", difficulty: "guest" });
  var auth = c.stops.find(function (s) {
    return s.id === "auth";
  });
  var r = N.evaluateExit(auth, { lane: 0, tags: [] }, { host: "home.lab", course: c });
  assert.equal(r.ok, false);
  assert.match(r.line, /authelia/);
});

test("origin 404 is the win", function () {
  var c = N.buildCourse({ path: "/nope", host: "home.lab", difficulty: "guest" });
  var origin = c.stops.find(function (s) {
    return s.id === "origin";
  });
  var r = N.evaluateExit(origin, { lane: c.dockerCorrect, tags: ["A", "SNI"] }, { path: "/nope", course: c });
  assert.equal(r.ending, "origin404");
  assert.match(r.line, /the box is fine/);
});

test("wrong container becomes polite 200", function () {
  var c = N.buildCourse({ path: "/nope", host: "home.lab", difficulty: "guest" });
  var origin = c.stops.find(function (s) {
    return s.id === "origin";
  });
  var r = N.evaluateExit(
    origin,
    { lane: c.dockerWrong, tags: ["A", "SNI", "WRONG_APP"] },
    { path: "/nope", course: c },
  );
  assert.equal(r.ending, "wrongApp");
  assert.match(r.line, /200 OK/);
});

test("docker dead mouth is 502", function () {
  var c = N.buildCourse({ path: "/jellyfin/x", host: "jellyfin.lab", difficulty: "homelab" });
  var dock = c.stops.find(function (s) {
    return s.id === "docker";
  });
  var r = N.evaluateExit(dock, { lane: c.dockerDead, tags: ["A", "SNI"] }, { course: c });
  assert.equal(r.ok, false);
  assert.match(r.line, /backend up/);
});

test("splice window", function () {
  var c = N.buildCourse({ path: "/", host: "home.lab" });
  var dns = c.stops.find(function (s) {
    return s.id === "dns";
  });
  assert.equal(N.inSplice(dns, dns.worldZ + dns.spliceAt + 1), true);
  assert.equal(N.inSplice(dns, dns.worldZ + 2), false);
});

test("hitBox jump/duck", function () {
  assert.equal(N.hitBox("jump", false, 0), true);
  assert.equal(N.hitBox("jump", false, 1.2), false);
  assert.equal(N.hitBox("duck", true, 0), false);
  assert.equal(N.hitBox("duck", false, 0), true);
});

test("gateOpen needs tag or any-of", function () {
  assert.equal(N.gateOpen({ need: "A" }, []), false);
  assert.equal(N.gateOpen({ need: "A" }, ["A"]), true);
  assert.equal(N.gateOpen({ needAny: ["SNI", "PADLOCK"] }, ["A"]), false);
  assert.equal(N.gateOpen({ needAny: ["SNI", "PADLOCK"] }, ["PADLOCK"]), true);
});

test("dns puts an A-gate on every lane after the pickup", function () {
  var c = N.buildCourse({ path: "/", host: "home.lab", difficulty: "guest" });
  var dns = c.stops.find(function (s) {
    return s.id === "dns";
  });
  var a = dns.pickups.find(function (p) {
    return p.id === "A";
  });
  assert.equal(dns.gates.length, 3);
  dns.gates.forEach(function (g) {
    assert.equal(g.need, "A");
    assert.ok(g.from > a.at, "gate must sit after the A record");
    assert.ok(g.from > dns.spliceAt + dns.spliceLen);
  });
});

test("router OVL is a WG gate, public WAN is not walled", function () {
  var lab = N.buildCourse({ path: "/", host: "home.lab", difficulty: "guest" });
  var pub = N.buildCourse({ path: "/", host: "example.com", difficulty: "guest" });
  var labR = lab.stops.find(function (s) {
    return s.id === "router";
  });
  var pubR = pub.stops.find(function (s) {
    return s.id === "router";
  });
  assert.equal((labR.walls || []).length, 0);
  assert.ok(
    labR.gates.some(function (g) {
      return g.lane === 2 && g.need === "WG";
    }),
  );
  assert.ok(
    labR.gates.some(function (g) {
      return g.lane === 0 && g.need === "WG";
    }),
  );
  assert.equal(
    pubR.gates.filter(function (g) {
      return g.lane === 0;
    }).length,
    0,
  );
});

test("tls WAN/LAN gates need SNI, OVL has none", function () {
  var c = N.buildCourse({ path: "/", host: "home.lab", difficulty: "homelab" });
  var tls = c.stops.find(function (s) {
    return s.id === "tls";
  });
  assert.equal(tls.gates.length, 2);
  tls.gates.forEach(function (g) {
    assert.ok(g.lane === 0 || g.lane === 1);
    assert.ok(g.needAny.indexOf("SNI") >= 0);
  });
});
