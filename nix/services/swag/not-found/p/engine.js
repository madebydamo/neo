/* Neo 404 — game loop, input, physics, collisions, HUD, endings. */
(function (root) {
  "use strict";

  var N = root.Neo404;
  if (!N || !N.buildCourse) throw new Error("shared.js and course.js must load before engine.js");

  var STEP = 1 / 60;
  var SWIPE_PX = 36;
  var DUCK_S = 0.55;
  var ORIGIN_HOLD = 1.2;
  var INTRO_DUR = 3.6;
  var MAX_PART = 80;
  var HAZARD_R = 0.85;
  var PICKUP_R = 0.9;
  var BOUNCE_R = 0.9;
  var ALT_PATHS = ["/jellyfin/web", "/photos/api", "/hass", "/git", "/vault/login"];

  var TRANSIT_TITLE = {
    sinkhole: "NXDOMAIN",
    nxdomain: "NXDOMAIN",
    refused: "REFUSED",
    wanAcl: "403",
    closedPort: "CLOSED",
    timeout: "TIMEOUT",
    tls: "TLS",
    noRouter: "404",
    auth401: "401",
    badGateway: "502",
    oom: "OOM",
    rst: "RST",
    insulation: "DROP",
    cat: "RST",
    banhammer: "403",
    expired: "TLS",
    hsts: "HSTS",
    ratelimit: "429",
    unhealthy: "502",
    restarting: "502",
  };

  var els = {};
  var reducedMotion = false;
  var mqReduce = null;
  var booted = false;
  var rafId = 0;
  var lastTs = 0;
  var acc = 0;
  var clock = 0;

  var path = "/";
  var host = "home.lab";
  var difficultyId = "guest";
  var difficulty = N.DIFFICULTY.guest;
  var course = null;

  var started = false;
  var overlayKind = "start";
  var runTime = 0;
  var score = 0;
  var speed = 0;
  var stop = null;
  var stopIndex = 0;
  var pulseLane = null;
  var flash = 0;
  var shakeMag = 0;
  var shake = { x: 0, y: 0 };
  var ending = null;
  var endHold = 0;
  var pendingEnd = null;
  var particles = [];
  var hopQueued = false;
  var duckHeld = false;
  var duckTimer = 0;
  var changing = false;
  var laneFromX = 0;
  var laneT = 1;
  var prevX = 0;
  var laneVel = 0;
  var collected = {};
  var bounceUsed = {};
  var mouthUsed = {};
  var lastTagsSig = "";
  var lastJournalStop = "";
  var diedOnce = false;
  var reachedProxy = false;
  var checkpointReady = false;
  var checkpoint = null;
  var pointer = null;
  var intro = { on: false, t: 0, dur: INTRO_DUR };

  var packet = {
    x: 0,
    y: 0,
    z: 4,
    lane: 1,
    laneVel: 0,
    h: N.PHYS.standH,
    ducking: false,
    vy: 0,
    path: "/",
    tags: [],
    alive: true,
    capePhase: 0,
  };

  var drawState = {
    packet: packet,
    course: null,
    stop: null,
    speed: 0,
    time: 0,
    pulseLane: null,
    shake: shake,
    flash: 0,
    ending: null,
    particles: particles,
    reducedMotion: false,
    collected: [],
    intro: { on: false, t: 0, dur: INTRO_DUR, path: "/", host: "" },
  };

  function $(id) {
    return document.getElementById(id);
  }

  function readReduced() {
    try {
      return !!(mqReduce && mqReduce.matches);
    } catch (e) {
      return false;
    }
  }

  function queryDiff() {
    try {
      var q = new URLSearchParams(location.search || "");
      var d = q.get("d");
      if (d && N.DIFFICULTY[d]) return d;
    } catch (e) {}
    return null;
  }

  function queryAtStop() {
    try {
      var id = new URLSearchParams(location.search || "").get("at");
      if (!id || !course) return null;
      return findStop(id);
    } catch (e) {
      return null;
    }
  }

  function queryPlay() {
    try {
      return new URLSearchParams(location.search || "").get("play") === "1";
    } catch (e) {
      return false;
    }
  }

  function queryIntroT() {
    try {
      var v = new URLSearchParams(location.search || "").get("intro");
      if (v == null || v === "") return null;
      var n = parseFloat(v);
      return isFinite(n) ? n : null;
    } catch (e) {
      return null;
    }
  }

  function storedDiff() {
    try {
      var d = localStorage.getItem("neo404.difficulty");
      if (d && N.DIFFICULTY[d]) return d;
    } catch (e) {}
    return "guest";
  }

  function writeDiff(id) {
    try {
      localStorage.setItem("neo404.difficulty", id);
    } catch (e) {}
  }

  function requestPath() {
    var p = "/";
    try {
      p = location.pathname || "/";
      var search = location.search || "";
      if (!search || search === "?") return p;
      var params = new URLSearchParams(search);
      params.delete("v");
      params.delete("d");
      params.delete("play");
      params.delete("at");
      params.delete("intro");
      var rest = params.toString();
      if (rest) return p + "?" + rest;
    } catch (e) {}
    return p;
  }

  function requestHost() {
    try {
      return location.host || "home.lab";
    } catch (e) {
      return "home.lab";
    }
  }

  function resolveDifficulty() {
    difficultyId = queryDiff() || storedDiff() || "guest";
    difficulty = N.DIFFICULTY[difficultyId] || N.DIFFICULTY.guest;
    difficultyId = difficulty.id;
  }

  function rebuildCourse(nextPath) {
    if (nextPath) path = nextPath;
    course = N.buildCourse({ path: path, host: host, difficulty: difficultyId });
    packet.path = path;
  }

  function findStop(id) {
    var i;
    if (!course) return null;
    for (i = 0; i < course.stops.length; i++) {
      if (course.stops[i].id === id) return course.stops[i];
    }
    return null;
  }

  function nearestLane(x) {
    var i;
    var best = 0;
    var bestD = Infinity;
    var d;
    for (i = 0; i < 3; i++) {
      d = Math.abs(x - N.PHYS.laneX[i]);
      if (d < bestD) {
        bestD = d;
        best = i;
      }
    }
    return best;
  }

  function laneName(lane) {
    var L = N.LANES[lane] || N.LANES[1];
    return L.key;
  }

  function grounded() {
    return packet.y <= 0 && packet.vy <= 0;
  }

  function hasTag(id) {
    return N.hasTag(packet.tags, id);
  }

  function addTag(id) {
    if (!id || hasTag(id)) return;
    packet.tags.push(id);
  }

  function spawnParticles(n, color, at) {
    var i;
    var p;
    var z = at && at.z != null ? at.z : packet.z;
    var x = at && at.x != null ? at.x : packet.x;
    var y = at && at.y != null ? at.y : packet.y + packet.h * 0.45;
    for (i = 0; i < n; i++) {
      if (particles.length >= MAX_PART) particles.shift();
      p = {
        x: x + (Math.random() - 0.5) * 0.45,
        y: y + Math.random() * 0.35,
        z: z + (Math.random() - 0.5) * 0.4,
        vx: (Math.random() - 0.5) * 3.2,
        vy: 1.4 + Math.random() * 3.4,
        life: 0.35 + Math.random() * 0.5,
        color: color,
        size: 0.06 + Math.random() * 0.1,
      };
      particles.push(p);
    }
  }

  function updateParticles(dt) {
    var i;
    var p;
    for (i = particles.length - 1; i >= 0; i--) {
      p = particles[i];
      p.life -= dt;
      p.x += (p.vx || 0) * dt;
      p.y += (p.vy || 0) * dt;
      p.vy = (p.vy || 0) - 6 * dt;
      if (p.life <= 0) particles.splice(i, 1);
    }
  }

  function resetPacket(opts) {
    var useCp = opts && opts.checkpoint && checkpointReady && checkpoint;
    var proxy = useCp ? findStop("proxy") : null;
    var lane = 1;
    var z = 4;
    var tags = [];
    var sc = 0;
    if (useCp && proxy) {
      z = proxy.worldZ + 2;
      tags = checkpoint.tags.slice();
      lane = typeof checkpoint.lane === "number" ? checkpoint.lane : 1;
      sc = checkpoint.score || 0;
    }
    packet.x = N.PHYS.laneX[lane];
    packet.y = 0;
    packet.z = z;
    packet.lane = lane;
    packet.laneVel = 0;
    packet.h = N.PHYS.standH;
    packet.ducking = false;
    packet.vy = 0;
    packet.path = path;
    packet.tags = tags;
    packet.alive = true;
    packet.capePhase = 0;
    prevX = packet.x;
    laneVel = 0;
    changing = false;
    laneT = 1;
    laneFromX = packet.x;
    hopQueued = false;
    duckHeld = false;
    duckTimer = 0;
    collected = {};
    bounceUsed = {};
    mouthUsed = {};
    particles.length = 0;
    flash = 0;
    shakeMag = 0;
    shake.x = 0;
    shake.y = 0;
    ending = null;
    endHold = 0;
    pendingEnd = null;
    pulseLane = null;
    score = sc;
    runTime = 0;
    lastTagsSig = "";
    lastJournalStop = "";
    stop = N.stopAt(course, packet.z);
    stopIndex = course.stops.indexOf(stop);
    if (stopIndex < 0) stopIndex = 0;
    speed = 0;
  }

  function hideOverlays() {
    if (els.start) els.start.hidden = true;
    if (els.end) els.end.hidden = true;
    overlayKind = null;
  }

  function showStart() {
    if (els.end) els.end.hidden = true;
    if (els.start) els.start.hidden = false;
    overlayKind = "start";
  }

  function applyEndDom(spec) {
    if (els.endKicker) els.endKicker.textContent = spec.kicker;
    if (els.endTitle) {
      els.endTitle.textContent = spec.title;
      els.endTitle.className = spec.cls;
    }
    if (els.endBlurb) els.endBlurb.textContent = spec.blurb;
    if (els.end) els.end.hidden = false;
    if (els.start) els.start.hidden = true;
    overlayKind = "end";
  }

  function showEnd(spec) {
    pendingEnd = spec;
    if (spec.delay && spec.delay > 0) {
      endHold = spec.delay;
      return;
    }
    endHold = 0;
    applyEndDom(spec);
  }

  function journal(text) {
    if (els.journal) els.journal.textContent = text || "";
  }

  function paintHud() {
    var key = laneName(packet.lane);
    var tagSig;
    var i;
    var id;
    var meta;
    var chip;
    var title;
    if (els.path) els.path.textContent = path;
    if (els.host) els.host.textContent = "Host: " + host;
    if (els.lane) {
      els.lane.textContent = key;
      els.lane.setAttribute("data-lane", key);
    }
    title = stop && (stop.title || stop.id) ? stop.title || stop.id : "socket";
    if (els.stop) els.stop.textContent = title;
    tagSig = packet.tags.join(",");
    if (tagSig !== lastTagsSig && els.tags) {
      lastTagsSig = tagSig;
      els.tags.textContent = "";
      for (i = 0; i < packet.tags.length; i++) {
        id = packet.tags[i];
        meta = N.PICKUP_META[id];
        chip = document.createElement("div");
        chip.className = "chip";
        chip.textContent = meta ? meta.chip : id === "WRONG_APP" ? "wrong app" : id;
        els.tags.appendChild(chip);
      }
    }
    if (els.score) {
      if (started) els.score.textContent = Math.round(score) + " · " + runTime.toFixed(1) + "s";
      else els.score.textContent = "";
    }
  }

  function goHome() {
    var a = els.home;
    if (a && a.href) {
      location.href = a.href;
      return;
    }
    location.href = "/";
  }

  function playing() {
    return started && packet.alive && !ending && !intro.on;
  }

  function skipIntro() {
    if (!intro.on) return;
    intro.on = false;
    intro.t = intro.dur;
    hopQueued = true;
  }

  function tryHop() {
    if (!playing()) return;
    if (grounded()) packet.vy = N.PHYS.jumpV;
  }

  function holdDuck(on) {
    duckHeld = !!on;
  }

  function pulseDuck() {
    duckTimer = DUCK_S;
  }

  function tryLane(dir) {
    var dest;
    if (!playing()) return;
    if (reducedMotion) return;
    if (!stop || !N.inSplice(stop, packet.z)) return;
    dest = N.clamp(packet.lane + dir, 0, 2);
    startLaneChange(dest);
  }

  function startLaneChange(dest) {
    dest = N.clamp(dest | 0, 0, 2);
    if (dest === packet.lane) return;
    laneFromX = packet.x;
    laneT = 0;
    changing = true;
    packet.lane = dest;
  }

  function currentSpeed() {
    var d = difficulty || N.DIFFICULTY.guest;
    return d.speed + stopIndex * d.speedPerStop;
  }

  function failLine(key, stopObj, fallback) {
    var line;
    if (stopObj && stopObj.onDie && stopObj.onDie[key]) return stopObj.onDie[key];
    line = N.FAIL_LINES[key];
    if (line) return line;
    return fallback || key;
  }

  function snapshotProxy() {
    if (reachedProxy) return;
    reachedProxy = true;
    checkpoint = {
      tags: packet.tags.slice(),
      score: score,
      lane: packet.lane,
    };
  }

  function die(key, line, stopObj) {
    var hops;
    var blurb;
    var title;
    if (!packet.alive || ending) return;
    packet.alive = false;
    ending = N.ENDINGS.transit;
    flash = 1;
    shakeMag = reducedMotion ? 0 : 16;
    spawnParticles(18, "#ff3b6b");
    diedOnce = true;
    if (reachedProxy) checkpointReady = true;
    line = line || failLine(key, stopObj, N.FAIL_LINES[key]);
    hops = stopIndex;
    blurb = line;
    if (stopObj && stopObj.title) blurb += "\nreached " + stopObj.title + " · hop " + hops;
    title = TRANSIT_TITLE[key] || String(key || "RST").toUpperCase();
    journal(line);
    showEnd({
      kicker: "died in transit",
      title: title,
      cls: "dead",
      blurb: blurb,
      delay: 0,
    });
  }

  function win(kind, line) {
    if (ending) return;
    ending = kind;
    packet.alive = true;
    if (kind === N.ENDINGS.origin404) {
      score += 300;
      writeDiff("homelab");
      if (difficultyId === "guest") {
        difficultyId = "homelab";
        difficulty = N.DIFFICULTY.homelab;
      }
      journal(line);
      showEnd({
        kicker: "application 404",
        title: "404",
        cls: "origin",
        blurb: line,
        delay: ORIGIN_HOLD,
      });
      return;
    }
    if (kind === N.ENDINGS.wrongApp) {
      score += 50;
      journal(line);
      showEnd({
        kicker: "wrong place",
        title: "200",
        cls: "wrong",
        blurb: line,
        delay: 0,
      });
    }
  }

  function exitCtx() {
    return { host: host, path: path, course: course };
  }

  function onStopEnter(next) {
    stop = next;
    stopIndex = course.stops.indexOf(stop);
    if (stopIndex < 0) stopIndex = 0;
    if (stop && stop.id === "proxy") snapshotProxy();
    if (stop && stop.onEnter && lastJournalStop !== stop.id) {
      lastJournalStop = stop.id;
      journal(stop.onEnter);
    }
  }

  function crossBoundary(prev) {
    var result;
    if (!prev) return;
    result = N.evaluateExit(prev, packet, exitCtx());
    if (!result) return;
    if (!result.ok) {
      die(result.key, result.line, prev);
      return;
    }
    score += 100;
    if (result.tag) addTag(result.tag);
    if (result.ending) win(result.ending, result.line);
  }

  function collideStop(seg) {
    var lz;
    var grace;
    var i;
    var hz;
    var w;
    var pit;
    var pk;
    var b;
    var m;
    var key;
    var near;
    var meta;
    if (!seg || !packet.alive || ending) return;
    lz = N.localZ(seg, packet.z);
    near = nearestLane(packet.x);
    grace = speed * N.PHYS.graceS;

    if (lz >= grace) {
      for (i = 0; i < (seg.hazards || []).length; i++) {
        hz = seg.hazards[i];
        if (near !== hz.lane) continue;
        if (Math.abs(lz - hz.at) >= HAZARD_R) continue;
        if (N.hitBox(hz.kind, packet.ducking, packet.y)) {
          die(hz.name || hz.kind, failLine(hz.name || hz.kind, seg), seg);
          return;
        }
      }
    }

    for (i = 0; i < (seg.walls || []).length; i++) {
      w = seg.walls[i];
      if (near !== w.lane) continue;
      if (lz >= w.from && lz <= w.to) {
        die(w.name, failLine(w.name, seg), seg);
        return;
      }
    }

    for (i = 0; i < (seg.gates || []).length; i++) {
      w = seg.gates[i];
      if (near !== w.lane) continue;
      if (lz < w.from || lz > w.to) continue;
      if (N.gateOpen(w, packet.tags)) continue;
      die(w.name, failLine(w.name, seg), seg);
      return;
    }

    for (i = 0; i < (seg.pits || []).length; i++) {
      pit = seg.pits[i];
      if (near !== pit.lane) continue;
      if (lz >= pit.from && lz <= pit.to) {
        die(pit.name, failLine(pit.name, seg), seg);
        return;
      }
    }

    for (i = 0; i < (seg.pickups || []).length; i++) {
      pk = seg.pickups[i];
      key = seg.id + ":" + pk.at + ":" + pk.id;
      if (collected[key]) continue;
      if (near !== pk.lane) continue;
      if (Math.abs(lz - pk.at) >= PICKUP_R) continue;
      collected[key] = true;
      if (!hasTag(pk.id)) {
        addTag(pk.id);
        score += 50;
      }
      meta = N.PICKUP_META[pk.id];
      spawnParticles(8, (N.LANES[pk.lane] && N.LANES[pk.lane].color) || "#ffd166");
      if (meta) journal("got " + meta.chip);
    }

    for (i = 0; i < (seg.bounce || []).length; i++) {
      b = seg.bounce[i];
      key = seg.id + ":b:" + b.at + ":" + b.lane;
      if (bounceUsed[key]) continue;
      if (near !== b.lane) continue;
      if (Math.abs(lz - b.at) >= BOUNCE_R) continue;
      bounceUsed[key] = true;
      packet.vy = N.PHYS.jumpV * 1.15;
    }

    for (i = 0; i < (seg.mouths || []).length; i++) {
      m = seg.mouths[i];
      key = seg.id + ":m:" + m.lane;
      if (mouthUsed[key]) continue;
      if (near !== m.lane) continue;
      if (lz < m.from || lz > m.to) continue;
      mouthUsed[key] = true;
      if (m.kind === "dead") {
        die("badGateway", failLine("badGateway", seg), seg);
        return;
      }
      if (m.kind === "wrong") addTag("WRONG_APP");
    }
  }

  function collide() {
    var next;
    if (!course || !stop) return;
    collideStop(stop);
    if (!packet.alive || ending) return;
    next = course.stops[stopIndex + 1];
    if (next) collideStop(next);
  }

  function updatePulse() {
    var lz;
    var lead;
    pulseLane = null;
    if (!stop || !started) return;
    if (!difficulty || !difficulty.pulseMs) return;
    if (stop.spliceAt < 0) return;
    lz = N.localZ(stop, packet.z);
    lead = speed * (difficulty.pulseMs / 1000);
    if (lz > stop.spliceAt - lead && lz <= stop.spliceAt + stop.spliceLen) {
      pulseLane = stop.correctLane;
    }
  }

  function step(dt) {
    var targetH;
    var destX;
    var dur;
    var prevStop;
    var next;
    var dx;

    clock += dt;
    packet.capePhase = clock;
    updateParticles(dt);

    if (flash > 0) flash = Math.max(0, flash - dt * 1.8);
    if (shakeMag > 0.15 && !reducedMotion) {
      shake.x = (Math.random() - 0.5) * 2 * shakeMag;
      shake.y = (Math.random() - 0.5) * 2 * shakeMag;
      shakeMag *= Math.exp(-dt * 8);
    } else {
      shake.x = 0;
      shake.y = 0;
      shakeMag = 0;
    }

    if (pendingEnd && endHold > 0) {
      endHold -= dt;
      if (endHold <= 0) {
        endHold = 0;
        applyEndDom(pendingEnd);
      }
    }

    if (!started || !course) {
      speed = 0;
      packet.z = 4;
      packet.h = N.PHYS.standH;
      return;
    }

    if (started && packet.alive && !ending && !intro.on) runTime += dt;

    if (intro.on) {
      intro.t += dt;
      speed = 0;
      packet.z = 3.4;
      packet.y = 0;
      packet.vy = 0;
      packet.h = N.PHYS.standH;
      packet.ducking = false;
      packet.laneVel = 0;
      if (intro.t >= intro.dur) {
        intro.on = false;
        intro.t = intro.dur;
        hopQueued = true;
      }
      return;
    }

    if (!playing()) {
      speed = started ? currentSpeed() : 0;
      packet.laneVel = 0;
      updatePulse();
      return;
    }

    stop = N.stopAt(course, packet.z);
    stopIndex = course.stops.indexOf(stop);
    if (stopIndex < 0) stopIndex = 0;
    speed = currentSpeed();

    if (reducedMotion && stop && N.inSplice(stop, packet.z) && packet.lane !== stop.correctLane) {
      startLaneChange(stop.correctLane);
    }

    if (hopQueued) {
      hopQueued = false;
      tryHop();
    }

    if (duckTimer > 0) duckTimer = Math.max(0, duckTimer - dt);

    if (changing) {
      dur = Math.max(N.PHYS.laneChangeMs / 1000, 1e-4);
      laneT += dt / dur;
      destX = N.PHYS.laneX[packet.lane];
      if (laneT >= 1) {
        laneT = 1;
        changing = false;
        packet.x = destX;
      } else {
        packet.x = N.lerp(laneFromX, destX, laneT);
      }
    } else {
      packet.x = N.PHYS.laneX[packet.lane];
    }

    packet.vy -= N.PHYS.gravity * dt;
    packet.y += packet.vy * dt;
    if (packet.y < 0) {
      packet.y = 0;
      packet.vy = 0;
    }

    packet.ducking = grounded() && (duckHeld || duckTimer > 0);
    targetH = packet.ducking ? N.PHYS.duckH : N.PHYS.standH;
    packet.h += (targetH - packet.h) * Math.min(1, dt * 16);

    prevStop = stop;
    packet.z += speed * dt;

    dx = packet.x - prevX;
    laneVel = N.clamp(dx / Math.max(dt, 1e-4) / 4, -1, 1);
    packet.laneVel = laneVel;
    prevX = packet.x;

    collide();
    if (!packet.alive || ending) return;

    next = N.stopAt(course, packet.z);
    if (prevStop && packet.z >= prevStop.endZ) {
      crossBoundary(prevStop);
      if (!packet.alive || ending) return;
      onStopEnter(next);
    } else if (next && next !== prevStop) {
      crossBoundary(prevStop);
      if (!packet.alive || ending) return;
      onStopEnter(next);
    }

    updatePulse();
  }

  function rendererDraw() {
    var R = N.Renderer;
    var i;
    var collectedIds;
    if (!R || typeof R.draw !== "function") return;
    collectedIds = [];
    for (i = 0; i < packet.tags.length; i++) {
      if (N.PICKUP_META[packet.tags[i]]) collectedIds.push(packet.tags[i]);
    }
    drawState.packet = packet;
    drawState.course = course;
    drawState.stop = stop;
    drawState.speed = speed;
    drawState.time = started ? runTime : clock;
    drawState.pulseLane = pulseLane;
    drawState.shake = shake;
    drawState.flash = flash;
    drawState.ending = ending;
    drawState.particles = particles;
    drawState.reducedMotion = reducedMotion;
    drawState.collected = collectedIds;
    drawState.intro = { on: intro.on, t: intro.t, dur: intro.dur, path: path, host: host };
    R.draw(drawState);
  }

  function scheduleFrame() {
    if (root.requestAnimationFrame) rafId = requestAnimationFrame(frame);
    else rafId = setTimeout(function () {
      frame(N.nowMs());
    }, 16);
  }

  function frame(ts) {
    var dt;
    scheduleFrame();
    if (typeof ts !== "number") ts = N.nowMs();
    if (!lastTs) lastTs = ts;
    dt = (ts - lastTs) / 1000;
    lastTs = ts;
    if (dt > 0.25) dt = 0.25;
    if (dt < 0) dt = 0;
    acc += dt;
    while (acc >= STEP) {
      step(STEP);
      acc -= STEP;
    }
    paintHud();
    rendererDraw();
  }

  function startRun(opts) {
    var at;
    resolveDifficulty();
    if (!course || (opts && opts.rebuild)) rebuildCourse(opts && opts.path);
    resetPacket(opts);
    started = true;
    hideOverlays();
    at = queryAtStop();
    if (at && !(opts && opts.checkpoint)) {
      packet.z = at.worldZ + 8;
      packet.x = N.PHYS.laneX[at.correctLane];
      packet.lane = at.correctLane;
      prevX = packet.x;
      if (at.id !== "socket") {
        addTag("A");
        addTag("SNI");
        addTag("PADLOCK");
      }
    }
    onStopEnter(N.stopAt(course, packet.z));
    speed = currentSpeed();
    var skipCin =
      (opts && (opts.checkpoint || opts.skipIntro)) ||
      !!queryAtStop() ||
      reducedMotion;
    var introSeek = queryIntroT();
    if (skipCin) {
      intro.on = false;
      intro.t = intro.dur;
      hopQueued = !(opts && opts.checkpoint);
    } else {
      intro.on = true;
      intro.t = introSeek == null ? 0 : N.clamp(introSeek, 0, intro.dur);
      packet.z = 3.4;
      packet.y = 0;
      packet.vy = 0;
      hopQueued = false;
      speed = 0;
      journal("tap the page");
      if (intro.t >= intro.dur) skipIntro();
    }
  }

  function retry() {
    var useCp = checkpointReady && ending === N.ENDINGS.transit;
    startRun({
      checkpoint: useCp,
      rebuild: !useCp && course && course.difficulty !== difficultyId,
    });
  }

  function anotherPath() {
    var i;
    var pool = [];
    var pick;
    var extra = "/nope/" + (N.hashStr(String(N.nowMs()) + path) % 10000);
    for (i = 0; i < ALT_PATHS.length; i++) {
      if (ALT_PATHS[i] !== path) pool.push(ALT_PATHS[i]);
    }
    if (extra !== path) pool.push(extra);
    pick = pool[Math.floor(Math.random() * pool.length)] || extra;
    diedOnce = false;
    reachedProxy = false;
    checkpointReady = false;
    checkpoint = null;
    startRun({ rebuild: true, path: pick, checkpoint: false });
  }

  function isGameKey(code) {
    return (
      code === "Space" ||
      code === "ArrowUp" ||
      code === "ArrowDown" ||
      code === "ArrowLeft" ||
      code === "ArrowRight" ||
      code === "KeyW" ||
      code === "KeyA" ||
      code === "KeyS" ||
      code === "KeyD"
    );
  }

  function onKeyDown(e) {
    var code = e.code;
    if (code === "Escape") {
      goHome();
      return;
    }
    if (!isGameKey(code)) return;
    e.preventDefault();
    if (e.repeat) return;
    if (code === "Space") {
      if (!started) {
        startRun();
        return;
      }
      if (intro.on) {
        skipIntro();
        return;
      }
      if (!playing()) return;
      hopQueued = true;
      duckHeld = true;
      return;
    }
    if (!started) return;
    if (!playing()) return;
    if (code === "ArrowUp" || code === "KeyW") {
      hopQueued = true;
      return;
    }
    if (code === "ArrowDown" || code === "KeyS") {
      holdDuck(true);
      return;
    }
    if (code === "ArrowLeft" || code === "KeyA") {
      tryLane(-1);
      return;
    }
    if (code === "ArrowRight" || code === "KeyD") {
      tryLane(1);
    }
  }

  function onKeyUp(e) {
    var code = e.code;
    if (code === "Space") {
      duckHeld = false;
      return;
    }
    if (code === "ArrowDown" || code === "KeyS") holdDuck(false);
  }

  function gestureFromPointer(dx, dy) {
    var ax = Math.abs(dx);
    var ay = Math.abs(dy);
    if (ax < SWIPE_PX && ay < SWIPE_PX) return "tap";
    if (ax > ay * 1.05) return dx < 0 ? "left" : "right";
    return dy < 0 ? "up" : "down";
  }

  function applyGesture(kind) {
    if (kind === "tap" || kind === "up") {
      hopQueued = true;
      return;
    }
    if (kind === "down") {
      pulseDuck();
      return;
    }
    if (kind === "left") tryLane(-1);
    if (kind === "right") tryLane(1);
  }

  function onPointerDown(e) {
    if (e.button != null && e.button !== 0) return;
    if (!started) {
      if (e.target && e.target.closest && e.target.closest("a")) return;
      startRun();
      pointer = null;
      return;
    }
    if (intro.on) {
      skipIntro();
      pointer = null;
      return;
    }
    if (!playing()) return;
    if (e.target && e.target.closest && e.target.closest("a,button,.overlay")) return;
    pointer = { x: e.clientX, y: e.clientY, type: e.pointerType || "mouse", id: e.pointerId };
    if (pointer.type !== "touch" && pointer.type !== "pen") {
      hopQueued = true;
      pointer = null;
    }
  }

  function onPointerUp(e) {
    var g;
    var dx;
    var dy;
    if (!pointer || pointer.id !== e.pointerId) return;
    dx = e.clientX - pointer.x;
    dy = e.clientY - pointer.y;
    g = gestureFromPointer(dx, dy);
    pointer = null;
    if (!playing()) return;
    applyGesture(g);
  }

  function onPointerCancel() {
    pointer = null;
  }

  function bindInputs() {
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerCancel);
    if (els.btnStart) {
      els.btnStart.addEventListener("click", function (e) {
        e.preventDefault();
        startRun();
      });
    }
    if (els.btnRetry) {
      els.btnRetry.addEventListener("click", function (e) {
        e.preventDefault();
        retry();
      });
    }
    if (els.btnAnother) {
      els.btnAnother.addEventListener("click", function (e) {
        e.preventDefault();
        anotherPath();
      });
    }
  }

  function bindResize() {
    var onResize = function () {
      var R = N.Renderer;
      if (R && typeof R.resize === "function") R.resize();
    };
    window.addEventListener("resize", onResize);
    if (window.visualViewport) window.visualViewport.addEventListener("resize", onResize);
  }

  function cacheEls() {
    els.canvas = $("stage");
    els.start = $("overlay-start");
    els.end = $("overlay-end");
    els.btnStart = $("btn-start");
    els.btnRetry = $("btn-retry");
    els.btnAnother = $("btn-another");
    els.path = $("hud-path");
    els.host = $("hud-host");
    els.lane = $("hud-lane");
    els.stop = $("hud-stop");
    els.tags = $("hud-tags");
    els.journal = $("hud-journal");
    els.score = $("hud-score");
    els.home = $("home");
    els.endKicker = $("end-kicker");
    els.endTitle = $("end-title");
    els.endBlurb = $("end-blurb");
  }

  function initRenderer() {
    var R = N.Renderer;
    if (!R || typeof R.init !== "function" || !els.canvas) return;
    R.init(els.canvas);
    if (typeof R.resize === "function") R.resize();
  }

  function idleSetup() {
    resolveDifficulty();
    rebuildCourse(path);
    resetPacket();
    started = false;
    packet.z = 4;
    speed = 0;
    stop = course.stops[0];
    stopIndex = 0;
    showStart();
    paintHud();
    journal("tap to start the request");
  }

  function boot() {
    if (booted) return;
    booted = true;
    cacheEls();
    if (!els.canvas) return;
    try {
      mqReduce = window.matchMedia("(prefers-reduced-motion: reduce)");
      if (mqReduce.addEventListener) {
        mqReduce.addEventListener("change", function () {
          reducedMotion = readReduced();
        });
      } else if (mqReduce.addListener) {
        mqReduce.addListener(function () {
          reducedMotion = readReduced();
        });
      }
    } catch (e) {}
    reducedMotion = readReduced();
    path = requestPath();
    host = requestHost();
    initRenderer();
    bindResize();
    bindInputs();
    idleSetup();
    lastTs = 0;
    acc = 0;
    scheduleFrame();
    if (queryPlay()) startRun({ rebuild: true });
  }

  var api = {
    boot: boot,
    start: startRun,
    retry: retry,
  };

  root.Neo404 = Object.assign(root.Neo404 || {}, { Engine: api });
  if (typeof module !== "undefined" && module.exports) module.exports = root.Neo404;

  if (typeof document !== "undefined") {
    if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", boot);
    else boot();
  }
})(typeof globalThis !== "undefined" ? globalThis : this);
