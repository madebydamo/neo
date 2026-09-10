/* Neo 404 — Subway-Surfers third-person canvas renderer. Canvas 2D only. */
(function (root) {
  "use strict";

  var N = root.Neo404 || {};
  var Z_NEAR = 0.4;
  var DRAW_DIST = 78;
  var CABLE_W = 0.72;
  var VOID = "#06050c";
  var FOG_HEX = "#07060c";
  var COPPER = "#c47a3a";
  var COPPER_HOT = "#e8a056";
  var FIBER = "#3df0c2";
  var PHOS = "#d4ff9a";
  var GOLD = "#ffd166";
  var DANGER = "#ff3b6b";
  var METAL = "#161821";
  var JACKET = "#12141c";
  var THEME_GLOW = {
    socket: ["rgba(255,176,82,0.32)", "rgba(196,122,58,0.08)"],
    dns: ["rgba(90,170,255,0.26)", "rgba(40,80,140,0.07)"],
    router: ["rgba(232,160,86,0.28)", "rgba(196,122,58,0.08)"],
    ingress: ["rgba(61,240,194,0.24)", "rgba(255,106,61,0.07)"],
    tls: ["rgba(70,255,150,0.28)", "rgba(20,90,55,0.08)"],
    proxy: ["rgba(255,209,102,0.26)", "rgba(61,240,194,0.07)"],
    auth: ["rgba(179,136,255,0.28)", "rgba(255,59,107,0.06)"],
    docker: ["rgba(70,150,255,0.26)", "rgba(179,136,255,0.07)"],
    origin: ["rgba(255,209,102,0.42)", "rgba(232,160,86,0.14)"],
    void: ["rgba(61,240,194,0.16)", "rgba(179,136,255,0.05)"],
  };
  var CAPE_BITS = ["GET", "Host:", "Accept", "HTTP/1.1", "Cookie", "X-Fwd"];
  var rgbCache = Object.create(null);

  var el = null;
  var ctx = null;
  var camX = 0;
  var lastPktZ = 0;
  var lastTime = 0;
  var camReady = false;
  var shards = [];
  var deathArmed = false;
  var spriteBuf = [];
  var listening = false;

  var cam = { x: 0, y: 1.85, z: -5.4, roll: 0 };
  var focal = 200;
  var cx = 0;
  var cy = 0;
  var W = 1;
  var H = 1;

  function clamp(v, lo, hi) {
    return v < lo ? lo : v > hi ? hi : v;
  }

  function lerp(a, b, t) {
    return a + (b - a) * t;
  }

  function lanes() {
    return N.LANES || [
      { color: "#ff6a3d", glow: "rgba(255,106,61,0.85)", key: "WAN" },
      { color: "#3df0c2", glow: "rgba(61,240,194,0.85)", key: "LAN" },
      { color: "#b388ff", glow: "rgba(179,136,255,0.9)", key: "OVL" },
    ];
  }

  function laneX(i) {
    var xs = (N.PHYS && N.PHYS.laneX) || [-1.62, 0, 1.62];
    return xs[i] || 0;
  }

  function laneColor(i) {
    var L = lanes();
    return (L[i] && L[i].color) || FIBER;
  }

  function laneGlow(i) {
    var L = lanes();
    return (L[i] && L[i].glow) || "rgba(61,240,194,0.85)";
  }

  function laneKey(i) {
    var L = lanes();
    return (L[i] && (L[i].key || L[i].title)) || ("L" + i);
  }

  function parseHex(hex) {
    var key = String(hex || "#000");
    var hit = rgbCache[key];
    if (hit) return hit;
    var h = key.charAt(0) === "#" ? key.slice(1) : key;
    if (h.length === 3) h = h.charAt(0) + h.charAt(0) + h.charAt(1) + h.charAt(1) + h.charAt(2) + h.charAt(2);
    var n = parseInt(h, 16);
    if (isNaN(n)) n = 0;
    hit = { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 };
    rgbCache[key] = hit;
    return hit;
  }

  function rgba(r, g, b, a) {
    return "rgba(" + (r | 0) + "," + (g | 0) + "," + (b | 0) + "," + a + ")";
  }

  function fogStyle(hex, rz, aMul) {
    var fog = clamp(rz / DRAW_DIST, 0, 1);
    var a = (1 - fog) * (aMul == null ? 1 : aMul);
    if (a <= 0.012) return null;
    var C = parseHex(hex);
    var F = parseHex(FOG_HEX);
    var t = fog;
    return rgba(C.r + (F.r - C.r) * t, C.g + (F.g - C.g) * t, C.b + (F.b - C.b) * t, a);
  }

  function hash01(a, b, c) {
    if (N.hashStr) return (N.hashStr(String(a) + ":" + b + ":" + (c || 0)) % 1000) / 1000;
    var n = Math.sin((a + 1.1) * 12.9898 + (b + 1.3) * 78.233 + (c || 0) * 4.17) * 43758.5453;
    return n - Math.floor(n);
  }

  function hasTag(tags, id) {
    if (!tags) return false;
    if (typeof tags.has === "function") return tags.has(id);
    return tags.indexOf(id) !== -1;
  }

  function isSplice(stop, z) {
    if (!stop) return false;
    if (N.inSplice) return N.inSplice(stop, z);
    if (stop.spliceAt < 0) return false;
    var lz = z - (stop.worldZ || 0);
    return lz >= stop.spliceAt && lz <= stop.spliceAt + (stop.spliceLen || 0);
  }

  function project(x, y, z) {
    var rz = z - cam.z;
    if (rz < Z_NEAR) return null;
    var s = focal / rz;
    return {
      sx: cx + (x - cam.x) * s,
      sy: cy - (y - cam.y) * s,
      s: s,
      rz: rz,
    };
  }

  function spr(p, mul) {
    if (!p) return 1;
    mul = mul == null ? 1 : mul;
    var minS = focal / (H > W * 1.18 ? 8.8 : 11.5);
    return Math.max(p.s, minS) * mul;
  }

  function drawDangerPad(lane, z, kind) {
    var x = laneX(lane);
    var rz = z - cam.z;
    if (rz < 1.2 || rz > 62) return;
    var col = kind === "duck" ? GOLD : kind === "wall" ? "#ff6a3d" : DANGER;
    var hw = 0.46;
    quad(
      project(x - hw, 0.125, z - 0.55),
      project(x + hw, 0.125, z - 0.55),
      project(x + hw, 0.125, z + 0.55),
      project(x - hw, 0.125, z + 0.55),
      fogStyle(col, rz, 0.55),
    );
    ctx.save();
    ctx.globalCompositeOperation = "lighter";
    quad(
      project(x - hw * 0.7, 0.14, z - 0.2),
      project(x + hw * 0.7, 0.14, z - 0.2),
      project(x + hw * 0.7, 0.14, z + 0.2),
      project(x - hw * 0.7, 0.14, z + 0.2),
      fogStyle("#fff", rz, 0.18),
    );
    ctx.restore();
    var chev = ((z * 3) | 0) % 2 === 0;
    if (chev) {
      quad(
        project(x - 0.22, 0.15, z - 0.18),
        project(x, 0.15, z + 0.22),
        project(x + 0.22, 0.15, z - 0.18),
        project(x, 0.15, z - 0.02),
        fogStyle("#fff6d8", rz, 0.4),
      );
    }
  }

  function quad(p0, p1, p2, p3, fill, stroke, lw) {
    if (!p0 || !p1 || !p2 || !p3 || !fill) return;
    ctx.beginPath();
    ctx.moveTo(p0.sx, p0.sy);
    ctx.lineTo(p1.sx, p1.sy);
    ctx.lineTo(p2.sx, p2.sy);
    ctx.lineTo(p3.sx, p3.sy);
    ctx.closePath();
    ctx.fillStyle = fill;
    ctx.fill();
    if (stroke) {
      ctx.strokeStyle = stroke;
      ctx.lineWidth = lw || 1;
      ctx.stroke();
    }
  }

  function zStep(rz) {
    if (rz > 48) return 2.4;
    if (rz > 28) return 1.35;
    if (rz > 14) return 0.95;
    return 0.72;
  }

  function dprCap() {
    var d = (typeof window !== "undefined" && window.devicePixelRatio) || 1;
    return d > 2 ? 2 : d < 1 ? 1 : d;
  }

  function resize() {
    if (!el) return;
    var dpr = dprCap();
    var cw = el.clientWidth || 0;
    var ch = el.clientHeight || 0;
    if (!cw || !ch) {
      cw = (el.width && dpr ? el.width / dpr : 0) || 390;
      ch = (el.height && dpr ? el.height / dpr : 0) || 720;
    }
    var tw = Math.max(1, Math.floor(cw * dpr));
    var th = Math.max(1, Math.floor(ch * dpr));
    if (el.width !== tw) el.width = tw;
    if (el.height !== th) el.height = th;
  }

  function init(canvas) {
    el = canvas || null;
    ctx = el && el.getContext ? el.getContext("2d") : null;
    camReady = false;
    shards.length = 0;
    deathArmed = false;
    if (!el) return;
    resize();
    if (!listening && typeof window !== "undefined") {
      listening = true;
      window.addEventListener("resize", resize);
      if (window.visualViewport) window.visualViewport.addEventListener("resize", resize);
    }
  }

  function visibleStops(state) {
    var out = [];
    var course = state.course;
    var z = (state.packet && state.packet.z) || 0;
    var i;
    var s;
    if (course && course.stops && course.stops.length) {
      for (i = 0; i < course.stops.length; i++) {
        s = course.stops[i];
        if (s.endZ < z - 6) continue;
        if (s.worldZ > z + DRAW_DIST) break;
        out.push(s);
        if (out.length >= 3) break;
      }
    }
    if (!out.length && state.stop) out.push(state.stop);
    return out;
  }

  function stopAtZ(stops, z, fallback) {
    var i;
    var s;
    for (i = 0; i < stops.length; i++) {
      s = stops[i];
      if (z >= (s.worldZ || 0) && z < (s.endZ != null ? s.endZ : (s.worldZ || 0) + (s.length || 0))) return s;
    }
    if (N.stopAt && fallback && fallback.stops) return N.stopAt(fallback, z);
    return stops[0] || null;
  }

  function pitOn(stops, lane, z) {
    var i;
    var j;
    var s;
    var p;
    var a;
    var b;
    for (i = 0; i < stops.length; i++) {
      s = stops[i];
      if (!s.pits) continue;
      for (j = 0; j < s.pits.length; j++) {
        p = s.pits[j];
        if (p.lane !== lane) continue;
        a = (s.worldZ || 0) + p.from;
        b = (s.worldZ || 0) + p.to;
        if (z >= a && z <= b) return p;
      }
    }
    return null;
  }

  function fitFont(text, maxW, maxSize, minSize) {
    var size = maxSize;
    var t = String(text || "");
    minSize = minSize || 9;
    ctx.font = "700 " + size + "px ui-monospace, SF Mono, Menlo, monospace";
    while (size > minSize && ctx.measureText(t).width > maxW) {
      size -= 1;
      ctx.font = "700 " + size + "px ui-monospace, SF Mono, Menlo, monospace";
    }
    return size;
  }

  function pulseLaneOn(state, lane) {
    if (state.pulseLane !== 0 && state.pulseLane !== 1 && state.pulseLane !== 2) return false;
    if (state.pulseLane !== lane) return false;
    var ms = (state.time || 0) * 1000;
    return ms % 400 < 210;
  }

  function drawBackdrop(theme) {
    var g = THEME_GLOW[theme] || THEME_GLOW.void;
    ctx.fillStyle = VOID;
    ctx.fillRect(0, 0, W, H);
    var rad = Math.min(W, H);
    var grd = ctx.createRadialGradient(cx, cy, rad * 0.02, cx, cy, rad * 0.62);
    grd.addColorStop(0, g[0]);
    grd.addColorStop(0.38, g[1]);
    grd.addColorStop(1, "rgba(6,5,12,0)");
    ctx.fillStyle = grd;
    ctx.fillRect(0, 0, W, H);
    ctx.save();
    ctx.globalCompositeOperation = "lighter";
    ctx.fillStyle = theme === "origin" ? "rgba(255,209,102,0.16)" : "rgba(212,255,154,0.06)";
    ctx.beginPath();
    ctx.arc(cx, cy, Math.max(6, rad * 0.018), 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  function wallX(side) {
    return side < 0 ? -4.08 : 4.08;
  }

  function drawTunnel(stops, state, time) {
    var z = cam.z + Z_NEAR + 0.2;
    var zFar = cam.z + DRAW_DIST;
    var theme = (state.stop && (state.stop.theme || state.stop.id)) || "void";
    while (z < zFar) {
      var rz = z - cam.z;
      var step = zStep(rz);
      var z2 = Math.min(z + step, zFar);
      var mid = (z + z2) * 0.5;
      var stop = stopAtZ(stops, mid, state.course) || state.stop;
      var localTheme = (stop && (stop.theme || stop.id)) || theme;
      drawCeiling(z, z2, rz, localTheme);
      drawWall(z, z2, rz, -1, time, localTheme);
      drawWall(z, z2, rz, 1, time, localTheme);
      drawFloor(z, z2, rz, localTheme, state);
      if (localTheme === "tls" && ((mid * 4) | 0) % 7 === 0) drawTlsArch(mid, rz);
      if (localTheme === "dns" && ((mid * 3) | 0) % 11 === 0) drawGantry(mid, rz, time);
      z = z2;
    }
  }

  function drawCeiling(z, z2, rz, theme) {
    var y = theme === "origin" ? 5.4 : 4.72;
    var fill = fogStyle(theme === "origin" ? "#1a140c" : "#0a0b12", rz, 0.92);
    quad(project(-5.6, y, z), project(5.6, y, z), project(5.6, y, z2), project(-5.6, y, z2), fill);
    var tray = fogStyle("#2a241c", rz, 0.55);
    quad(project(-4.2, y - 0.18, z), project(-3.15, y - 0.18, z), project(-3.15, y - 0.18, z2), project(-4.2, y - 0.18, z2), tray);
    quad(project(3.15, y - 0.18, z), project(4.2, y - 0.18, z), project(4.2, y - 0.18, z2), project(3.15, y - 0.18, z2), tray);
  }

  function drawFloor(z, z2, rz, theme, state) {
    var y = -0.22;
    var fill = fogStyle(theme === "origin" ? "#14100c" : "#09090f", rz, 0.95);
    quad(project(-3.7, y, z), project(3.7, y, z), project(3.7, y, z2), project(-3.7, y, z2), fill);
    var trench = fogStyle("#05040a", rz, 0.78);
    quad(project(-2.62, y + 0.02, z), project(2.62, y + 0.02, z), project(2.62, y + 0.02, z2), project(-2.62, y + 0.02, z2), trench);
    var slat = 0.92;
    var s0 = Math.floor(z / slat) * slat;
    var s;
    var odd;
    var slatRz;
    for (s = s0; s < z2 - 0.02; s += slat) {
      odd = (Math.abs(Math.round(s / slat)) & 1) === 1;
      slatRz = s - cam.z;
      quad(
        project(-3.55, y + 0.012, s),
        project(3.55, y + 0.012, s),
        project(3.55, y + 0.012, s + slat * 0.72),
        project(-3.55, y + 0.012, s + slat * 0.72),
        fogStyle(odd ? "#12141c" : "#0b0c14", slatRz, theme === "origin" ? 0.55 : 0.7),
      );
      quad(
        project(-3.55, y + 0.018, s),
        project(3.55, y + 0.018, s),
        project(3.55, y + 0.018, s + 0.07),
        project(-3.55, y + 0.018, s + 0.07),
        fogStyle(theme === "origin" ? "#3a2a14" : "#2a3348", slatRz, 0.45),
      );
    }
    var i;
    var hx;
    var col;
    var dash;
    var d0;
    var d;
    var on;
    var t = (state && state.time) || 0;
    for (i = 0; i < 3; i++) {
      hx = laneX(i);
      col = fogStyle(laneColor(i), rz, 0.1);
      quad(
        project(hx - 0.46, y + 0.035, z),
        project(hx + 0.46, y + 0.035, z),
        project(hx + 0.46, y + 0.035, z2),
        project(hx - 0.46, y + 0.035, z2),
        col,
      );
    }
    dash = 0.55;
    d0 = Math.floor(z / dash) * dash;
    for (i = 0; i < 2; i++) {
      hx = (laneX(i) + laneX(i + 1)) * 0.5;
      for (d = d0; d < z2; d += dash) {
        on = (Math.abs(Math.round(d / dash)) & 1) === 0;
        if (!on) continue;
        quad(
          project(hx - 0.035, y + 0.05, d),
          project(hx + 0.035, y + 0.05, d),
          project(hx + 0.035, y + 0.05, d + dash * 0.42),
          project(hx - 0.035, y + 0.05, d + dash * 0.42),
          fogStyle("#d4ff9a", d - cam.z, 0.22 + 0.08 * Math.sin(t * 6 + d)),
        );
      }
    }
    if (rz < 26) {
      var sweep = ((t * 18) % 22) + cam.z + 4;
      if (sweep > z && sweep < z2) {
        quad(
          project(-2.4, y + 0.06, sweep),
          project(2.4, y + 0.06, sweep),
          project(2.4, y + 0.06, sweep + 0.18),
          project(-2.4, y + 0.06, sweep + 0.18),
          fogStyle("#9ad4ff", rz, 0.16),
        );
      }
    }
  }

  function drawWall(z, z2, rz, side, time, theme) {
    var x = wallX(side);
    var xOuter = x + side * 1.35;
    var y0 = -0.22;
    var y1 = theme === "origin" ? 5.35 : 4.72;
    var metal = theme === "origin" ? "#1c1810" : METAL;
    var fill = fogStyle(metal, rz, 0.96);
    quad(project(x, y0, z), project(x, y1, z), project(x, y1, z2), project(x, y0, z2), fill);
    var edge = fogStyle("#0b0c12", rz, 0.5);
    quad(project(x, y0, z), project(xOuter, y0 * 0.4, z), project(xOuter, y0 * 0.4, z2), project(x, y0, z2), edge);
    var bay = fogStyle("#0c0d14", rz, 0.55);
    var u;
    for (u = 0.55; u < y1 - 0.4; u += 0.42) {
      var a = project(x, u, z);
      var b = project(x, u, z2);
      if (!a || !b) continue;
      ctx.strokeStyle = bay;
      ctx.lineWidth = Math.max(1, a.s * 0.012);
      ctx.beginPath();
      ctx.moveTo(a.sx, a.sy);
      ctx.lineTo(b.sx, b.sy);
      ctx.stroke();
    }
    var postEvery = 2.35;
    if (Math.floor(z / postEvery) !== Math.floor(z2 / postEvery) || z % postEvery < zStep(rz) * 0.6) {
      var pz = Math.round(z / postEvery) * postEvery;
      var post = fogStyle("#0a0b10", rz, 0.8);
      quad(
        project(x, y0, pz - 0.06),
        project(x, y1, pz - 0.06),
        project(x, y1, pz + 0.06),
        project(x, y0, pz + 0.06),
        post,
      );
    }
    if (rz < 52) {
      var k;
      var ledY;
      var on;
      var lc;
      for (k = 0; k < 3; k++) {
        ledY = 1.05 + k * 0.95;
        on = ledOn(z, side, k, time);
        lc = on ? (k === 2 ? DANGER : k === 1 ? GOLD : FIBER) : "#2a2e38";
        var lp0 = project(x + side * -0.04, ledY, z + 0.12);
        var lp1 = project(x + side * -0.04, ledY + 0.08, z + 0.12);
        var lp2 = project(x + side * -0.04, ledY + 0.08, z + 0.28);
        var lp3 = project(x + side * -0.04, ledY, z + 0.28);
        quad(lp0, lp1, lp2, lp3, fogStyle(lc, rz, on ? 0.95 : 0.35));
      }
    }
    if (theme === "proxy" && rz < 46) drawPatchPorts(x, side, z, z2, rz);
  }

  function ledOn(z, side, k, time) {
    var h = hash01((z / 2.35) | 0, side + 2, k);
    var phase = h * 2.8;
    var t = (time + phase) % (1.6 + h);
    if (h < 0.18) return t < 0.14;
    return h > 0.32;
  }

  function drawPatchPorts(x, side, z, z2, rz) {
    var row;
    var col;
    var colors = ["#ff6a3d", "#3df0c2", "#b388ff", "#ffd166", "#4ea3ff"];
    for (row = 0; row < 2; row++) {
      for (col = 0; col < 2; col++) {
        var y = 2.1 + row * 0.42;
        var zz = z + 0.18 + col * 0.32;
        if (zz > z2) continue;
        var c = colors[(row * 3 + col + ((z * 3) | 0)) % colors.length];
        quad(
          project(x + side * -0.05, y, zz),
          project(x + side * -0.05, y + 0.16, zz),
          project(x + side * -0.05, y + 0.16, zz + 0.14),
          project(x + side * -0.05, y, zz + 0.14),
          fogStyle(c, rz, 0.55),
        );
      }
    }
  }

  function drawGantry(z, rz, time) {
    var y = 3.55;
    var bar = fogStyle("#3a3f4c", rz, 0.8);
    quad(project(-4.05, y, z - 0.08), project(4.05, y, z - 0.08), project(4.05, y, z + 0.08), project(-4.05, y, z + 0.08), bar);
    quad(project(-4.05, y + 0.35, z - 0.08), project(4.05, y + 0.35, z - 0.08), project(4.05, y + 0.35, z + 0.08), project(-4.05, y + 0.35, z + 0.08), bar);
    var i;
    for (i = 0; i < 3; i++) {
      var hx = laneX(i);
      quad(
        project(hx - 0.06, 2.4, z),
        project(hx + 0.06, 2.4, z),
        project(hx + 0.06, y, z),
        project(hx - 0.06, y, z),
        fogStyle("#2c313c", rz, 0.7),
      );
    }
    var p = project(0, y + 0.18, z);
    if (p && rz < 40) {
      ctx.save();
      ctx.globalAlpha = clamp(1 - rz / 50, 0.15, 0.7);
      ctx.fillStyle = Math.sin(time * 3 + z) > 0.2 ? FIBER : "#4ea3ff";
      ctx.beginPath();
      ctx.arc(p.sx, p.sy, Math.max(1.5, p.s * 0.05), 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }
  }

  function drawTlsArch(z, rz) {
    var i;
    var segs = 6;
    var pts = [];
    var ang;
    var px;
    var py;
    var p;
    for (i = 0; i <= segs; i++) {
      ang = Math.PI * (i / segs);
      px = Math.cos(Math.PI - ang) * 3.35;
      py = 0.2 + Math.sin(ang) * 3.1;
      p = project(px, py, z);
      if (p) pts.push(p);
    }
    if (pts.length < 3) return;
    ctx.beginPath();
    ctx.moveTo(pts[0].sx, pts[0].sy);
    for (i = 1; i < pts.length; i++) ctx.lineTo(pts[i].sx, pts[i].sy);
    ctx.strokeStyle = fogStyle("#1e8f6a", rz, 0.55) || "transparent";
    ctx.lineWidth = Math.max(1.2, (pts[Math.floor(pts.length / 2)].s || 8) * 0.06);
    ctx.stroke();
    ctx.strokeStyle = fogStyle(PHOS, rz, 0.22) || "transparent";
    ctx.lineWidth = Math.max(0.6, (pts[Math.floor(pts.length / 2)].s || 8) * 0.025);
    ctx.stroke();
  }

  function drawCableSeg(lane, z, z2, jacket, spliced, strobe, time) {
    var x = laneX(lane);
    var hw = CABLE_W * 0.5;
    var yTop = 0.09;
    var ySide = -0.13;
    var rz = (z + z2) * 0.5 - cam.z;
    var col = laneColor(lane);
    var braid = jacket ? JACKET : COPPER;
    var braidHot = jacket ? "#1c202b" : COPPER_HOT;
    var left = fogStyle("#3a2414", rz, jacket ? 0.35 : 0.85);
    var right = fogStyle(braidHot, rz, jacket ? 0.4 : 0.9);
    var top = fogStyle(strobe ? col : braid, rz, strobe ? 0.95 : jacket ? 0.72 : 0.96);
    quad(project(x - hw, yTop, z), project(x - hw, ySide, z), project(x - hw, ySide, z2), project(x - hw, yTop, z2), left);
    quad(project(x + hw, yTop, z), project(x + hw, ySide, z), project(x + hw, ySide, z2), project(x + hw, yTop, z2), right);
    quad(project(x - hw, yTop, z), project(x + hw, yTop, z), project(x + hw, yTop, z2), project(x - hw, yTop, z2), top);
    if (!jacket) {
      var coreW = 0.155;
      var coreY = yTop + 0.015;
      var pulse = 0.55 + 0.45 * Math.sin((z * 0.42 - time * 7.2) + lane);
      var core = fogStyle(strobe ? PHOS : col, rz, (strobe ? 1 : 0.72) * pulse);
      quad(
        project(x - coreW, coreY, z),
        project(x + coreW, coreY, z),
        project(x + coreW, coreY, z2),
        project(x - coreW, coreY, z2),
        core,
      );
      if (rz < 40 && ((Math.floor(z * 2.2 + time * 14) % 3) === 0)) {
        quad(
          project(x - hw * 0.42, yTop + 0.028, z),
          project(x + hw * 0.42, yTop + 0.028, z),
          project(x + hw * 0.18, yTop + 0.028, Math.min(z + (z2 - z) * 0.55, z2)),
          project(x - hw * 0.18, yTop + 0.028, Math.min(z + (z2 - z) * 0.55, z2)),
          fogStyle(col, rz, 0.28),
        );
      }
      if (rz < 36) {
        var spec = fogStyle("#fff6d8", rz, 0.22);
        quad(
          project(x - hw * 0.55, yTop + 0.02, z),
          project(x - hw * 0.18, yTop + 0.02, z),
          project(x - hw * 0.18, yTop + 0.02, z2),
          project(x - hw * 0.55, yTop + 0.02, z2),
          spec,
        );
      }
    } else {
      var pin = fogStyle(col, rz, 0.28);
      quad(
        project(x - 0.04, yTop + 0.012, z),
        project(x + 0.04, yTop + 0.012, z),
        project(x + 0.04, yTop + 0.012, z2),
        project(x - 0.04, yTop + 0.012, z2),
        pin,
      );
    }
    if (spliced && rz < 48) {
      var chev = ((z * 1.7) | 0) % 2 === 0;
      var ch = fogStyle(chev ? col : PHOS, rz, strobe ? 0.55 : 0.32);
      var inset = hw * 0.62;
      quad(
        project(x - inset, yTop + 0.03, z),
        project(x, yTop + 0.03, z + (z2 - z) * 0.45),
        project(x + inset, yTop + 0.03, z),
        project(x, yTop + 0.03, z + (z2 - z) * 0.12),
        ch,
      );
    }
  }

  function drawCables(stops, state, time) {
    var pktLane = (state.packet && state.packet.lane);
    if (pktLane !== 0 && pktLane !== 1 && pktLane !== 2) pktLane = 1;
    var z = cam.z + Z_NEAR + 0.12;
    var zFar = cam.z + DRAW_DIST;
    while (z < zFar) {
      var rz = z - cam.z;
      var step = zStep(rz);
      var z2 = Math.min(z + step, zFar);
      var mid = (z + z2) * 0.5;
      var stop = stopAtZ(stops, mid, state.course) || state.stop;
      var spliced = isSplice(stop, mid);
      var lane;
      for (lane = 0; lane < 3; lane++) {
        if (pitOn(stops, lane, mid)) {
          drawPitVoid(lane, z, z2, rz, time);
          continue;
        }
        var jacket = !spliced && lane !== pktLane;
        drawCableSeg(lane, z, z2, jacket, spliced, pulseLaneOn(state, lane) || (spliced && lane === pktLane && rz < 22), time);
      }
      if (spliced && rz < 34) drawSpliceArrows(pktLane, mid, time);
      z = z2;
    }
    drawNearLaneMarks(pktLane, (state.packet && state.packet.z) || 0);
  }

  function drawPitVoid(lane, z, z2, rz, time) {
    var x = laneX(lane);
    var y = -0.35;
    var hole = fogStyle("#030208", rz, 0.95);
    quad(project(x - 0.5, 0.02, z), project(x + 0.5, 0.02, z), project(x + 0.5, 0.02, z2), project(x - 0.5, 0.02, z2), hole);
    quad(project(x - 0.38, y, z), project(x + 0.38, y, z), project(x + 0.38, y, z2), project(x - 0.38, y, z2), fogStyle("#000", rz, 0.9));
    var p = project(x, 0.35, (z + z2) * 0.5);
    if (p && rz < 48) {
      ctx.save();
      ctx.globalAlpha = clamp(0.9 * (1 - rz / 52), 0.35, 0.95);
      ctx.fillStyle = DANGER;
      ctx.font = "800 " + Math.max(12, spr(p) * 0.28) + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.shadowColor = DANGER;
      ctx.shadowBlur = 16;
      var drift = Math.sin(time * 2 + z) * p.s * 0.08;
      ctx.fillText("NO ROUTER", p.sx, p.sy + drift);
      ctx.restore();
    }
    var lip = fogStyle(COPPER, rz, 0.55);
    quad(project(x - 0.5, 0.09, z), project(x - 0.34, 0.09, z), project(x - 0.34, 0.09, z2), project(x - 0.5, 0.09, z2), lip);
    quad(project(x + 0.34, 0.09, z), project(x + 0.5, 0.09, z), project(x + 0.5, 0.09, z2), project(x + 0.34, 0.09, z2), lip);
  }

  function drawSpliceArrows(pktLane, z, time) {
    var lane;
    for (lane = 0; lane < 3; lane++) {
      if (lane === pktLane) continue;
      var x = laneX(lane);
      var bob = 0.04 * Math.sin(time * 8 + lane);
      var p0 = project(x, 0.22 + bob, z);
      var p1 = project(x - 0.18, 0.1 + bob, z + 0.35);
      var p2 = project(x + 0.18, 0.1 + bob, z + 0.35);
      if (!p0 || !p1 || !p2) continue;
      ctx.beginPath();
      ctx.moveTo(p0.sx, p0.sy);
      ctx.lineTo(p1.sx, p1.sy);
      ctx.lineTo(p2.sx, p2.sy);
      ctx.closePath();
      ctx.fillStyle = fogStyle(laneColor(lane), p0.rz, 0.72) || laneColor(lane);
      ctx.fill();
    }
  }

  function drawNearLaneMarks(pktLane, pktZ) {
    var lane;
    for (lane = 0; lane < 3; lane++) {
      var z = pktZ + 3.2;
      var p = project(laneX(lane), 0.14, z);
      if (!p || p.rz > 18) continue;
      var fs = Math.max(9, Math.min(18, p.s * 0.16));
      ctx.save();
      ctx.globalAlpha = clamp(1.15 - p.rz / 16, 0.25, 0.9);
      ctx.font = (lane === pktLane ? "800 " : "600 ") + fs + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.lineWidth = 3;
      ctx.strokeStyle = "rgba(6,5,12,0.75)";
      ctx.strokeText(laneKey(lane), p.sx, p.sy);
      ctx.fillStyle = laneColor(lane);
      ctx.fillText(laneKey(lane), p.sx, p.sy);
      ctx.restore();
    }
  }

  function pushSprite(z, layer, fn) {
    spriteBuf.push({ z: z, layer: layer, draw: fn });
  }

  function collectSprites(stops, state, time, reduced) {
    spriteBuf.length = 0;
    var i;
    for (i = 0; i < stops.length; i++) collectStop(stops[i], state, time, reduced);
    if (state.particles && state.particles.length) collectParticles(state);
  }

  function collectStop(stop, state, time, reduced) {
    if (!stop) return;
    var wz = stop.worldZ || 0;
    var pkt = state.packet || {};
    var pktZ = pkt.z || 0;
    addThemeSprites(stop, state, time, reduced);
    var i;
    var o;
    var z;
    var walls = stop.walls || [];
    for (i = 0; i < walls.length; i++) {
      o = walls[i];
      z = wz + (o.from != null ? o.from : 0);
      pushSprite(z, 1, drawWallBlock.bind(null, stop, o, time));
    }
    var gates = stop.gates || [];
    for (i = 0; i < gates.length; i++) {
      o = gates[i];
      z = wz + (o.from != null ? o.from : 0);
      pushSprite(z, 2, drawGate.bind(null, o, z, state, time));
    }
    var pits = stop.pits || [];
    for (i = 0; i < pits.length; i++) {
      o = pits[i];
      z = wz + (o.from != null ? o.from : 0) + 4;
      pushSprite(z, 0, drawPitFrags.bind(null, stop, o, time));
    }
    var bounce = stop.bounce || [];
    for (i = 0; i < bounce.length; i++) {
      o = bounce[i];
      z = wz + (o.at || 0);
      pushSprite(z, 4, drawBounce.bind(null, o, z, time, reduced));
    }
    var mouths = stop.mouths || [];
    for (i = 0; i < mouths.length; i++) {
      o = mouths[i];
      z = wz + (o.from != null ? o.from : 0);
      pushSprite(z, 2, drawMouth.bind(null, stop, o, time));
    }
    var hazards = stop.hazards || [];
    for (i = 0; i < hazards.length; i++) {
      o = hazards[i];
      z = wz + (o.at || 0);
      if (z < pktZ - 1.4 || z > pktZ + DRAW_DIST) continue;
      pushSprite(z, 5, drawHazard.bind(null, o, z, time, reduced));
    }
    var pickups = stop.pickups || [];
    for (i = 0; i < pickups.length; i++) {
      o = pickups[i];
      z = wz + (o.at || 0);
      if (isPickupGone(state, o, z, pktZ)) continue;
      pushSprite(z, 6, drawPickup.bind(null, o, z, time, reduced));
    }
    var signs = stop.signs || [];
    var signZ = wz + signLocalZ(stop);
    for (i = 0; i < signs.length; i++) {
      o = signs[i];
      z = o.at != null ? wz + o.at : signZ;
      if (z < pktZ - 2 || z > pktZ + DRAW_DIST) continue;
      pushSprite(z, 7, drawSign.bind(null, o, z, stop));
    }
    if ((stop.id === "origin" || stop.theme === "origin") && pktZ > wz - 8) {
      pushSprite(wz + (stop.length || 80) * 0.72, 3, drawOriginRoom.bind(null, stop, state, time));
    }
  }

  function signLocalZ(stop) {
    if (stop.spliceAt >= 0) return Math.max(10, stop.spliceAt - 8);
    return 16;
  }

  function isPickupGone(state, o, z, pktZ) {
    if (z < pktZ - 0.4) return true;
    if (state.collected && state.collected.indexOf(o.id) !== -1) return true;
    if (state.packet && hasTag(state.packet.tags, o.id) && z < pktZ + 0.7) return true;
    return false;
  }

  function addThemeSprites(stop, state, time, reduced) {
    var id = stop.id || stop.theme;
    var wz = stop.worldZ || 0;
    var i;
    if (id === "socket") {
      for (i = 0; i < 7; i++) {
        (function (n) {
          var z = wz + 5 + n * 4.2;
          pushSprite(z, 3, function () {
            drawAddressShard(n, z, time, reduced);
          });
        })(i);
      }
    } else if (id === "router") {
      pushSprite(wz + 40, 3, function () {
        drawIspBox(wz + 40, time);
      });
    } else if (id === "ingress") {
      pushSprite(wz + 68, 3, function () {
        drawIngressDucts(stop, wz + 70, time);
      });
    } else if (id === "proxy") {
      for (i = 0; i < 6; i++) {
        (function (n) {
          var z = wz + 36 + n * 14;
          pushSprite(z, 3, function () {
            drawRouterCard(n, z, time, reduced);
          });
        })(i);
      }
    } else if (id === "auth") {
      pushSprite(wz + 78, 3, function () {
        drawTurnstile(wz + 78, time);
      });
    } else if (id === "dns") {
      pushSprite(wz + 52, 3, function () {
        drawBlackholeDecor(wz + 52, time);
      });
    }
  }

  function drawAddressShard(n, z, time, reduced) {
    var x = ((n % 3) - 1) * 1.7 + (hash01(n, 2, 1) - 0.5) * 0.4;
    var y = 1.4 + hash01(n, 3, 2) * 1.6 + (reduced ? 0 : Math.sin(time * 1.6 + n) * 0.08);
    var p = project(x, y, z);
    if (!p) return;
    var w = p.s * (1.1 + (n % 3) * 0.25);
    var h = p.s * 0.28;
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate((hash01(n, 4, 0) - 0.5) * 0.5);
    ctx.fillStyle = fogStyle("#1c2230", p.rz, 0.82) || METAL;
    roundRect(-w * 0.5, -h * 0.5, w, h, Math.max(2, p.s * 0.04));
    ctx.fill();
    ctx.strokeStyle = fogStyle("#8aa0b8", p.rz, 0.55) || "#889";
    ctx.lineWidth = 1;
    ctx.stroke();
    var bits = ["https://", "GET /", "Host", "chrome://", "404", "addr", "www"];
    ctx.fillStyle = fogStyle("#d7efe6", p.rz, 0.8) || "#d7efe6";
    ctx.font = "600 " + Math.max(8, h * 0.45) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(bits[n % bits.length], 0, 0);
    ctx.restore();
  }

  function drawIspBox(z, time) {
    var x = -3.15;
    var y0 = 0.05;
    var y1 = 1.55;
    var fill = fogStyle("#6a3b22", z - cam.z, 0.92);
    quad(project(x, y0, z - 0.4), project(x + 1.1, y0, z - 0.4), project(x + 1.1, y1, z - 0.4), project(x, y1, z - 0.4), fill);
    quad(project(x + 1.1, y0, z - 0.4), project(x + 1.1, y0, z + 0.5), project(x + 1.1, y1, z + 0.5), project(x + 1.1, y1, z - 0.4), fogStyle("#4a2818", z - cam.z, 0.9));
    var p = project(x + 0.55, y1 - 0.28, z - 0.38);
    if (!p) return;
    ctx.font = "700 " + Math.max(8, p.s * 0.12) + "px ui-sans-serif, sans-serif";
    ctx.fillStyle = fogStyle("#e8c9a0", p.rz, 0.9) || "#e8c9a0";
    ctx.textAlign = "center";
    ctx.fillText("ISP", p.sx, p.sy);
    var onW = (time * 3) % 2 < 1.2;
    var wan = project(x + 0.25, y1 - 0.55, z - 0.38);
    var lanP = project(x + 0.7, y1 - 0.55, z - 0.38);
    if (wan) {
      ctx.fillStyle = onW ? "#ff6a3d" : "#3a2018";
      ctx.beginPath();
      ctx.arc(wan.sx, wan.sy, Math.max(1.5, wan.s * 0.045), 0, Math.PI * 2);
      ctx.fill();
    }
    if (lanP) {
      ctx.fillStyle = FIBER;
      ctx.beginPath();
      ctx.arc(lanP.sx, lanP.sy, Math.max(1.5, lanP.s * 0.045), 0, Math.PI * 2);
      ctx.fill();
    }
  }

  function drawIngressDucts(stop, z, time) {
    var walls = stop.walls || [];
    var closed = {};
    var i;
    for (i = 0; i < walls.length; i++) closed[walls[i].lane] = true;
    var lane;
    for (lane = 0; lane < 3; lane++) {
      drawDuct(lane, z, !!closed[lane], time);
    }
  }

  function drawDuct(lane, z, sealed, time) {
    var x = laneX(lane);
    var p = project(x, 1.15, z);
    if (!p) return;
    var r = p.s * 0.72;
    ctx.save();
    ctx.beginPath();
    ctx.arc(p.sx, p.sy, r, 0, Math.PI * 2);
    ctx.fillStyle = sealed ? fogStyle("#1a1d26", p.rz, 0.92) : fogStyle("#07060c", p.rz, 0.9);
    ctx.fill();
    ctx.lineWidth = Math.max(2, p.s * 0.08);
    ctx.strokeStyle = fogStyle(sealed ? "#4a5160" : laneColor(lane), p.rz, 0.9) || laneColor(lane);
    ctx.stroke();
    ctx.font = "700 " + Math.max(9, p.s * 0.16) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillStyle = sealed ? "#8a93a3" : laneColor(lane);
    ctx.fillText(sealed ? "SEALED" : laneKey(lane), p.sx, p.sy);
    if (!sealed) {
      ctx.globalCompositeOperation = "lighter";
      ctx.strokeStyle = "rgba(61,240,194,0.28)";
      ctx.beginPath();
      ctx.arc(p.sx, p.sy, r * (0.7 + 0.08 * Math.sin(time * 4)), 0, Math.PI * 2);
      ctx.stroke();
    }
    ctx.restore();
  }

  function drawRouterCard(n, z, time, reduced) {
    var x = ((n % 3) - 1) * 1.5;
    var y = 1.7 + (n % 2) * 0.7 + (reduced ? 0 : Math.sin(time * 2 + n) * 0.1);
    var p = project(x, y, z);
    if (!p) return;
    var w = p.s * 0.7;
    var h = p.s * 0.38;
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate((reduced ? 0 : time * 0.4 + n) * 0.15);
    ctx.fillStyle = fogStyle("#141820", p.rz, 0.85) || METAL;
    roundRect(-w * 0.5, -h * 0.5, w, h, 3);
    ctx.fill();
    ctx.strokeStyle = fogStyle(GOLD, p.rz, 0.7) || GOLD;
    ctx.stroke();
    ctx.fillStyle = GOLD;
    ctx.font = "700 " + Math.max(8, h * 0.32) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(n % 2 ? "Host(" : "Path(", 0, 0);
    ctx.restore();
  }

  function drawTurnstile(z, time) {
    var x = laneX(0);
    var p = project(x, 1.15, z);
    if (!p) return;
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate(Math.sin(time * 0.3) * 0.08);
    var arm = p.s * 0.85;
    ctx.strokeStyle = fogStyle("#b388ff", p.rz, 0.85) || "#b388ff";
    ctx.lineWidth = Math.max(2, p.s * 0.06);
    ctx.beginPath();
    ctx.moveTo(0, p.s * 0.7);
    ctx.lineTo(0, -p.s * 0.2);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(-arm * 0.6, 0);
    ctx.lineTo(arm * 0.6, 0);
    ctx.moveTo(-arm * 0.2, -arm * 0.5);
    ctx.lineTo(arm * 0.2, arm * 0.5);
    ctx.stroke();
    ctx.fillStyle = DANGER;
    ctx.font = "700 " + Math.max(9, p.s * 0.18) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.fillText("401", 0, -p.s * 0.55);
    ctx.restore();
  }

  function drawBlackholeDecor(z, time) {
    var x = laneX(0);
    var p = project(x, 0.55, z);
    if (!p) return;
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate(time * 1.4);
    var k;
    ctx.strokeStyle = fogStyle("#4ea3ff", p.rz, 0.45) || "#4ea3ff";
    for (k = 0; k < 3; k++) {
      ctx.beginPath();
      ctx.ellipse(0, 0, p.s * (0.22 + k * 0.12), p.s * (0.1 + k * 0.06), k * 0.4, 0, Math.PI * 2);
      ctx.stroke();
    }
    ctx.restore();
  }

  function drawGate(gate, z, state, time) {
    var x = laneX(gate.lane);
    var tags = (state.packet && state.packet.tags) || state.collected || [];
    var open = N.gateOpen ? N.gateOpen(gate, tags) : false;
    var rz = z - cam.z;
    var hw = 0.5;
    var y0 = 0.08;
    var yTop = open ? 2.55 : 2.15;
    var yGap = open ? 1.15 : y0;
    var label = String(gate.label || gate.name || "GATE").toUpperCase();
    if (!open) drawDangerPad(gate.lane, z - 0.35, "wall");
    quad(
      project(x - hw, yGap, z),
      project(x + hw, yGap, z),
      project(x + hw, yTop, z),
      project(x - hw, yTop, z),
      fogStyle(open ? "#10221c" : "#2a1018", rz, open ? 0.72 : 0.96),
    );
    var b;
    var bars = open ? 3 : 6;
    for (b = 0; b < bars; b++) {
      var yy = yGap + 0.1 + b * (open ? 0.42 : 0.32);
      quad(
        project(x - hw + 0.04, yy, z - 0.02),
        project(x + hw - 0.04, yy, z - 0.02),
        project(x + hw - 0.04, yy + 0.12, z - 0.02),
        project(x - hw + 0.04, yy + 0.12, z - 0.02),
        fogStyle(open ? FIBER : b % 2 ? DANGER : GOLD, rz, open ? 0.45 : 0.9),
      );
    }
    quad(
      project(x - hw - 0.06, y0, z),
      project(x - hw + 0.08, y0, z),
      project(x - hw + 0.08, 2.6, z),
      project(x - hw - 0.06, 2.6, z),
      fogStyle("#2a303c", rz, 0.92),
    );
    quad(
      project(x + hw - 0.08, y0, z),
      project(x + hw + 0.06, y0, z),
      project(x + hw + 0.06, 2.6, z),
      project(x + hw - 0.08, 2.6, z),
      fogStyle("#2a303c", rz, 0.92),
    );
    var p = project(x, open ? 2.35 : 1.2, z - 0.03);
    if (!p) return;
    ctx.save();
    ctx.font = "800 " + Math.max(10, Math.min(24, p.s * 0.18)) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.lineWidth = 4;
    ctx.strokeStyle = "rgba(6,5,12,0.85)";
    ctx.strokeText(open ? "PASS" : label, p.sx, p.sy);
    ctx.fillStyle = open ? FIBER : DANGER;
    ctx.fillText(open ? "PASS" : label, p.sx, p.sy);
    ctx.restore();
    if (!open) {
      var led = project(x, 2.4, z - 0.02);
      if (led) {
        ctx.fillStyle = ((time * 4) | 0) % 2 === 0 ? DANGER : "#4a1520";
        ctx.beginPath();
        ctx.arc(led.sx, led.sy, Math.max(2, led.s * 0.05), 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  function drawWallBlock(stop, wall, time) {
    var wz = (stop.worldZ || 0) + (wall.from || 0);
    var z1 = (stop.worldZ || 0) + (wall.to != null ? wall.to : (wall.from || 0) + 12);
    var x = laneX(wall.lane);
    var hw = 0.52;
    var y0 = 0.08;
    var y1 = 2.55;
    var rz = wz - cam.z;
    drawDangerPad(wall.lane, wz - 0.4, "wall");
    var stripe = ((time * 6) | 0) % 2 === 0;
    var face = fogStyle(stripe ? "#3a1020" : "#241018", rz, 0.98);
    quad(project(x - hw, y0, wz), project(x + hw, y0, wz), project(x + hw, y1, wz), project(x - hw, y1, wz), face);
    var b;
    for (b = 0; b < 5; b++) {
      var yy = y0 + 0.12 + b * 0.46;
      quad(
        project(x - hw, yy, wz - 0.01),
        project(x + hw, yy, wz - 0.01),
        project(x + hw, yy + 0.2, wz - 0.01),
        project(x - hw, yy + 0.2, wz - 0.01),
        fogStyle(b % 2 ? DANGER : GOLD, rz, 0.85),
      );
    }
    quad(project(x + hw, y0, wz), project(x + hw, y0, Math.min(z1, wz + 6)), project(x + hw, y1, Math.min(z1, wz + 6)), project(x + hw, y1, wz), fogStyle("#101218", rz, 0.9));
    quad(project(x - hw, y0, wz), project(x + hw, y0, wz), project(x + hw, y0, Math.min(z1, wz + 6)), project(x - hw, y0, Math.min(z1, wz + 6)), fogStyle("#2a303c", rz, 0.8));
    var bolt = fogStyle("#8a93a3", rz, 0.7);
    quad(project(x - hw, y1 - 0.08, wz), project(x + hw, y1 - 0.08, wz), project(x + hw, y1, wz), project(x - hw, y1, wz), bolt);
    var p = project(x, 1.35, wz - 0.02);
    if (!p) return;
    var label = String(wall.name || "BLOCK").replace(/([A-Z])/g, " $1").trim().toUpperCase();
    ctx.save();
    ctx.font = "800 " + Math.max(10, Math.min(26, p.s * 0.2)) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.lineWidth = 4;
    ctx.strokeStyle = "rgba(6,5,12,0.85)";
    ctx.strokeText(label, p.sx, p.sy);
    ctx.fillStyle = DANGER;
    ctx.fillText(label, p.sx, p.sy);
    ctx.restore();
    var blink = ((time * 4) | 0) % 2 === 0;
    var led = project(x, y1 - 0.22, wz - 0.02);
    if (led) {
      ctx.fillStyle = blink ? DANGER : "#4a1520";
      ctx.beginPath();
      ctx.arc(led.sx, led.sy, Math.max(2, led.s * 0.05), 0, Math.PI * 2);
      ctx.fill();
    }
  }

  function drawPitFrags(stop, pit, time) {
    var z = (stop.worldZ || 0) + (pit.from || 0) + 6;
    drawDangerPad(pit.lane, (stop.worldZ || 0) + (pit.from || 0) + 1.2, "wall");
    var x = laneX(pit.lane);
    var i;
    var glyphs = ["4", "0", "4", "?"];
    for (i = 0; i < 4; i++) {
      var y = -0.2 - ((time * 0.7 + i * 0.33) % 1.6);
      var p = project(x + (i - 1.5) * 0.12, y, z + i * 1.4);
      if (!p) continue;
      ctx.globalAlpha = clamp(0.55 * (1 - p.rz / 50), 0.1, 0.5);
      ctx.fillStyle = DANGER;
      ctx.font = "700 " + Math.max(8, p.s * 0.2) + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.fillText(glyphs[i], p.sx, p.sy);
      ctx.globalAlpha = 1;
    }
  }

  function drawBounce(b, z, time, reduced) {
    var x = laneX(b.lane);
    var bob = reduced ? 0 : Math.sin(time * 7) * 0.04;
    var p0 = project(x, 0.12 + bob, z);
    var p1 = project(x - 0.32, 0.12 + bob, z + 0.55);
    var p2 = project(x + 0.32, 0.12 + bob, z + 0.55);
    var p3 = project(x, 0.55 + bob, z + 0.15);
    if (!p0 || !p3) return;
    ctx.beginPath();
    ctx.moveTo(p3.sx, p3.sy);
    ctx.lineTo(p1.sx, p1.sy);
    ctx.lineTo(p0.sx, p0.sy);
    ctx.lineTo(p2.sx, p2.sy);
    ctx.closePath();
    ctx.fillStyle = fogStyle(GOLD, p0.rz, 0.8) || GOLD;
    ctx.fill();
    ctx.save();
    ctx.globalCompositeOperation = "lighter";
    ctx.fillStyle = "rgba(255,209,102,0.35)";
    ctx.fill();
    ctx.restore();
    var t = project(x, 0.62 + bob, z);
    if (t) {
      ctx.fillStyle = "#06201a";
      ctx.font = "800 " + Math.max(8, t.s * 0.12) + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.fillText("https →", t.sx, t.sy);
    }
  }

  function drawMouth(stop, mouth, time) {
    var z = (stop.worldZ || 0) + (mouth.from || 0);
    var z2 = (stop.worldZ || 0) + (mouth.to != null ? mouth.to : mouth.from + 20);
    var x = laneX(mouth.lane);
    var hw = 0.62;
    var y0 = 0.08;
    var y1 = 2.35;
    var rz = z - cam.z;
    var kind = mouth.kind || "correct";
    var rim = kind === "dead" ? DANGER : kind === "wrong" ? FIBER : "#3a4250";
    var inner = kind === "dead" ? "#2a0810" : kind === "wrong" ? "#0a241c" : "#05060a";
    quad(project(x - hw, y0, z), project(x + hw, y0, z), project(x + hw, y1, z), project(x - hw, y1, z), fogStyle(inner, rz, 0.95));
    quad(project(x - hw - 0.08, y0, z), project(x - hw, y0, z), project(x - hw, y1, z), project(x - hw - 0.08, y1, z), fogStyle(rim, rz, 0.9));
    quad(project(x + hw, y0, z), project(x + hw + 0.08, y0, z), project(x + hw + 0.08, y1, z), project(x + hw, y1, z), fogStyle(rim, rz, 0.9));
    quad(project(x - hw - 0.08, y1, z), project(x + hw + 0.08, y1, z), project(x + hw + 0.08, y1 + 0.12, z), project(x - hw - 0.08, y1 + 0.12, z), fogStyle(rim, rz, 0.92));
    quad(project(x + hw, y0, z), project(x + hw, y0, z2), project(x + hw, y1, z2), project(x + hw, y1, z), fogStyle("#10141c", rz, 0.7));
    var p = project(x, y1 + 0.28, z - 0.04);
    if (p) {
      var name = String(mouth.name || kind);
      var fs = Math.max(11, Math.min(28, p.s * 0.22));
      ctx.save();
      ctx.font = "800 " + fs + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.lineWidth = 4;
      ctx.strokeStyle = "rgba(6,5,12,0.85)";
      ctx.strokeText(name, p.sx, p.sy);
      ctx.fillStyle = kind === "dead" ? DANGER : kind === "wrong" ? FIBER : GOLD;
      ctx.fillText(name, p.sx, p.sy);
      ctx.restore();
    }
    if (kind === "dead") {
      var c = project(x, 1.2, z - 0.05);
      if (c) {
        ctx.save();
        ctx.translate(c.sx, c.sy);
        ctx.rotate(time * 4);
        ctx.strokeStyle = DANGER;
        ctx.lineWidth = Math.max(2, c.s * 0.06);
        ctx.beginPath();
        ctx.arc(0, 0, c.s * 0.28, 0, Math.PI * 2);
        ctx.moveTo(-c.s * 0.16, -c.s * 0.16);
        ctx.lineTo(c.s * 0.16, c.s * 0.16);
        ctx.moveTo(c.s * 0.16, -c.s * 0.16);
        ctx.lineTo(-c.s * 0.16, c.s * 0.16);
        ctx.stroke();
        ctx.restore();
      }
    } else if (kind === "wrong") {
      var g = project(x, 1.1, z - 0.05);
      if (g) {
        ctx.fillStyle = fogStyle(FIBER, rz, 0.35) || FIBER;
        ctx.beginPath();
        ctx.arc(g.sx, g.sy, g.s * 0.2, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  function drawHazard(h, z, time, reduced) {
    var name = h.name || h.kind || "rst";
    var kind = h.kind || "jump";
    var x = laneX(h.lane);
    drawDangerPad(h.lane, z, kind);
    if (kind === "duck") drawDuckHazard(name, x, z, time, reduced);
    else drawJumpHazard(name, x, z, time, reduced);
  }

  function drawJumpHazard(name, x, z, time, reduced) {
    if (name === "sinkhole") return drawSinkhole(x, z, time);
    if (name === "cat") return drawCat(x, z, time, reduced);
    if (name === "rst") return drawSpike(x, z, time);
    if (name === "expired") return drawCert(x, z, time, reduced);
    if (name === "oom") return drawOom(x, z, time, reduced);
    if (name === "unhealthy") return drawUnhealthy(x, z, time);
    if (name === "restarting") return drawDoor(x, z, time);
    if (name === "fan") return drawFan(x, z, time, reduced);
    return drawSpike(x, z, time);
  }

  function drawDuckHazard(name, x, z, time, reduced) {
    if (name === "banhammer") return drawHammer(x, z, time, reduced);
    if (name === "hsts") return drawHsts(x, z, time);
    if (name === "ratelimit") return drawBoomArm(x, z, time, reduced);
    if (name === "413") return drawLowPipe(x, z);
    return drawBoomArm(x, z, time, reduced);
  }

  function drawSinkhole(x, z, time) {
    var p = project(x, 0.12, z);
    if (!p) return;
    var sc = spr(p);
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate(time * 1.8);
    var k;
    for (k = 5; k >= 0; k--) {
      ctx.beginPath();
      ctx.ellipse(0, 0, sc * (0.16 + k * 0.1), sc * (0.07 + k * 0.045), k * 0.3, 0, Math.PI * 2);
      ctx.fillStyle = k === 0 ? "#000" : fogStyle(k % 2 ? "#2a1040" : "#6aa8ff", p.rz, 0.45 + k * 0.08);
      ctx.fill();
    }
    ctx.restore();
    ctx.save();
    ctx.fillStyle = "#9ad4ff";
    ctx.font = "800 " + Math.max(10, sc * 0.16) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.shadowColor = "#4ea3ff";
    ctx.shadowBlur = 12;
    ctx.fillText("SINK", p.sx, p.sy - sc * 0.42);
    ctx.restore();
  }

  function drawCat(x, z, time, reduced) {
    var bob = reduced ? 0 : Math.sin(time * 6) * 0.03;
    var body = project(x, 0.48 + bob, z);
    if (!body) return;
    var sc = spr(body);
    var head = project(x + 0.14, 0.82 + bob, z);
    var tail = project(x - 0.42, 0.6 + bob, z);
    ctx.save();
    ctx.lineJoin = "round";
    ctx.fillStyle = fogStyle("#e39a3a", body.rz, 1) || "#e39a3a";
    ctx.strokeStyle = "#2a1208";
    ctx.lineWidth = Math.max(2, sc * 0.04);
    ctx.beginPath();
    ctx.ellipse(body.sx, body.sy, sc * 0.34, sc * 0.22, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    if (head) {
      ctx.beginPath();
      ctx.arc(head.sx, head.sy, sc * 0.18, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
      ctx.beginPath();
      ctx.moveTo(head.sx - sc * 0.14, head.sy - sc * 0.06);
      ctx.lineTo(head.sx - sc * 0.2, head.sy - sc * 0.34);
      ctx.lineTo(head.sx - sc * 0.02, head.sy - sc * 0.14);
      ctx.moveTo(head.sx + sc * 0.02, head.sy - sc * 0.12);
      ctx.lineTo(head.sx + sc * 0.16, head.sy - sc * 0.36);
      ctx.lineTo(head.sx + sc * 0.14, head.sy - sc * 0.05);
      ctx.fill();
      ctx.fillStyle = GOLD;
      ctx.shadowColor = GOLD;
      ctx.shadowBlur = 10;
      ctx.beginPath();
      ctx.arc(head.sx - sc * 0.05, head.sy, Math.max(2, sc * 0.035), 0, Math.PI * 2);
      ctx.arc(head.sx + sc * 0.06, head.sy, Math.max(2, sc * 0.035), 0, Math.PI * 2);
      ctx.fill();
      ctx.shadowBlur = 0;
    }
    if (tail) {
      ctx.strokeStyle = fogStyle("#e39a3a", body.rz, 1) || "#e39a3a";
      ctx.lineWidth = Math.max(3, sc * 0.07);
      ctx.beginPath();
      ctx.moveTo(body.sx - sc * 0.22, body.sy);
      ctx.quadraticCurveTo(tail.sx, tail.sy - sc * 0.35, tail.sx, tail.sy);
      ctx.stroke();
    }
    ctx.fillStyle = "#fff6ea";
    ctx.font = "800 " + Math.max(10, sc * 0.16) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.fillText("CAT", body.sx, body.sy + sc * 0.42);
    ctx.restore();
  }

  function drawSpike(x, z, time) {
    var p0 = project(x, 0.08, z);
    var tipY = 1.25 + 0.05 * Math.sin(time * 20);
    var p1 = project(x - 0.22, 0.08, z);
    var p2 = project(x + 0.22, 0.08, z);
    var p3 = project(x, tipY, z);
    if (!p0 || !p3) return;
    ctx.beginPath();
    ctx.moveTo(p1.sx, p1.sy);
    ctx.lineTo(p3.sx, p3.sy);
    ctx.lineTo(p2.sx, p2.sy);
    ctx.closePath();
    ctx.fillStyle = fogStyle("#e8eef8", p0.rz, 1) || "#eee";
    ctx.fill();
    ctx.strokeStyle = DANGER;
    ctx.lineWidth = Math.max(2.5, spr(p0) * 0.05);
    ctx.stroke();
    var t = project(x, 0.58, z);
    if (t) {
      ctx.fillStyle = DANGER;
      ctx.font = "800 " + Math.max(11, spr(t) * 0.18) + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.shadowColor = DANGER;
      ctx.shadowBlur = 12;
      ctx.fillText("RST", t.sx, t.sy);
      ctx.shadowBlur = 0;
    }
  }

  function drawCert(x, z, time, reduced) {
    var flap = reduced ? 0 : Math.sin(time * 9) * 0.18;
    var p = project(x, 0.85, z);
    if (!p) return;
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate(flap);
    var w = p.s * 0.42;
    var h = p.s * 0.55;
    ctx.fillStyle = fogStyle("#f4efe2", p.rz, 0.92) || "#eee";
    roundRect(-w * 0.5, -h * 0.5, w, h, 3);
    ctx.fill();
    ctx.strokeStyle = DANGER;
    ctx.lineWidth = 2;
    ctx.stroke();
    ctx.fillStyle = DANGER;
    ctx.font = "800 " + Math.max(9, p.s * 0.16) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText("CERT", 0, -h * 0.12);
    ctx.fillText("X", 0, h * 0.22);
    ctx.restore();
  }

  function drawOom(x, z, time, reduced) {
    var bob = reduced ? 0 : Math.sin(time * 3) * 0.05;
    var p = project(x, 0.45 + bob, z);
    if (!p) return;
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.fillStyle = fogStyle("#5a2040", p.rz, 0.92) || "#5a2040";
    ctx.beginPath();
    ctx.ellipse(0, 0, p.s * 0.34, p.s * 0.22, 0.2, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.ellipse(-p.s * 0.12, -p.s * 0.12, p.s * 0.16, p.s * 0.14, -0.4, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = DANGER;
    ctx.font = "700 " + Math.max(8, p.s * 0.14) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.fillText("OOM", 0, 0);
    ctx.restore();
  }

  function drawUnhealthy(x, z, time) {
    var p = project(x, 0.7, z);
    if (!p) return;
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate(time * 5);
    ctx.strokeStyle = DANGER;
    ctx.lineWidth = Math.max(2, p.s * 0.07);
    ctx.beginPath();
    ctx.arc(0, 0, p.s * 0.32, 0, Math.PI * 2);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(-p.s * 0.18, -p.s * 0.18);
    ctx.lineTo(p.s * 0.18, p.s * 0.18);
    ctx.moveTo(p.s * 0.18, -p.s * 0.18);
    ctx.lineTo(-p.s * 0.18, p.s * 0.18);
    ctx.stroke();
    ctx.restore();
  }

  function drawDoor(x, z, time) {
    var slide = (Math.sin(time * 3) * 0.5 + 0.5) * 0.35;
    var zf = z;
    quad(
      project(x - 0.4, 0.1, zf),
      project(x + 0.4, 0.1, zf),
      project(x + 0.4, 1.9, zf),
      project(x - 0.4, 1.9, zf),
      fogStyle("#1c2430", z - cam.z, 0.9),
    );
    quad(
      project(x - 0.38 + slide, 0.15, zf - 0.02),
      project(x + 0.05 + slide, 0.15, zf - 0.02),
      project(x + 0.05 + slide, 1.85, zf - 0.02),
      project(x - 0.38 + slide, 1.85, zf - 0.02),
      fogStyle("#4a5568", z - cam.z, 0.88),
    );
    var p = project(x, 2.05, zf);
    if (p) {
      ctx.fillStyle = GOLD;
      ctx.font = "700 " + Math.max(8, p.s * 0.12) + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.fillText("restarting", p.sx, p.sy);
    }
  }

  function drawFan(x, z, time, reduced) {
    var p = project(x, 0.7, z);
    if (!p) return;
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate(reduced ? 0.4 : time * 8);
    var i;
    ctx.fillStyle = fogStyle("#9aa3b2", p.rz, 0.88) || "#aaa";
    for (i = 0; i < 3; i++) {
      ctx.rotate((Math.PI * 2) / 3);
      ctx.beginPath();
      ctx.ellipse(p.s * 0.18, 0, p.s * 0.22, p.s * 0.07, 0.4, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.beginPath();
    ctx.arc(0, 0, p.s * 0.08, 0, Math.PI * 2);
    ctx.fillStyle = METAL;
    ctx.fill();
    ctx.restore();
  }

  function drawHammer(x, z, time, reduced) {
    drawDuckGate(x, z, time, "BAN", "#c45a2a");
    var swing = reduced ? 0.2 : Math.sin(time * 5) * 0.35;
    var p = project(x, 1.7, z);
    if (!p) return;
    var sc = spr(p);
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate(swing);
    ctx.strokeStyle = fogStyle("#6a4a2a", p.rz, 0.95) || "#6a4a2a";
    ctx.lineWidth = Math.max(4, sc * 0.08);
    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.lineTo(0, sc * 0.85);
    ctx.stroke();
    ctx.fillStyle = fogStyle("#c0c6d0", p.rz, 0.95) || "#ccc";
    roundRect(-sc * 0.32, sc * 0.72, sc * 0.64, sc * 0.24, 3);
    ctx.fill();
    ctx.restore();
  }

  function drawDuckGate(x, z, time, label, color) {
    var y = 1.08;
    var rz = z - cam.z;
    var bob = Math.sin(time * 9) * 0.03;
    quad(
      project(x - 0.55, y + bob, z - 0.12),
      project(x + 0.55, y + bob, z - 0.12),
      project(x + 0.55, y + 0.42 + bob, z - 0.12),
      project(x - 0.55, y + 0.42 + bob, z - 0.12),
      fogStyle(color, rz, 0.92),
    );
    quad(
      project(x - 0.55, y + 0.42 + bob, z - 0.12),
      project(x + 0.55, y + 0.42 + bob, z - 0.12),
      project(x + 0.55, 2.4, z - 0.12),
      project(x - 0.55, 2.4, z - 0.12),
      fogStyle("#0a0c12", rz, 0.55),
    );
    var p = project(x, y + 0.22 + bob, z);
    if (!p) return;
    ctx.save();
    ctx.fillStyle = "#fff6ea";
    ctx.font = "800 " + Math.max(12, spr(p) * 0.2) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.shadowColor = color;
    ctx.shadowBlur = 14;
    ctx.fillText(label, p.sx, p.sy);
    ctx.shadowBlur = 0;
    ctx.fillStyle = GOLD;
    ctx.font = "700 " + Math.max(10, spr(p) * 0.14) + "px ui-monospace, monospace";
    ctx.fillText("DUCK", p.sx, p.sy + spr(p) * 0.28);
    ctx.restore();
  }

  function drawHsts(x, z, time) {
    drawDuckGate(x, z, time, "HSTS", "#1e8f6a");
  }

  function drawBoomArm(x, z, time, reduced) {
    drawDuckGate(x, z, time, "429", "#ff9a3d");
    var ang = reduced ? 0.15 : Math.sin(time * 4.2) * 0.35;
    var p = project(x - 0.62, 1.7, z);
    if (!p) return;
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate(ang);
    ctx.strokeStyle = fogStyle("#ff9a3d", p.rz, 0.95) || "#ff9a3d";
    ctx.lineWidth = Math.max(4, spr(p) * 0.1);
    ctx.beginPath();
    ctx.moveTo(0, 0);
    ctx.lineTo(spr(p) * 1.05, spr(p) * 0.1);
    ctx.stroke();
    ctx.restore();
  }

  function drawLowPipe(x, z) {
    drawDuckGate(x, z, 0, "413", "#5a6578");
  }

  function drawPickup(o, z, time, reduced) {
    var x = laneX(o.lane);
    var bob = reduced ? 0 : Math.sin(time * 4.2 + z) * 0.1;
    var spin = reduced ? 0.3 : time * 2.4 + z;
    var y = 0.72 + bob;
    var p = project(x, y, z);
    if (!p) return;
    var meta = (N.PICKUP_META && N.PICKUP_META[o.id]) || { chip: o.id || "tag" };
    var sChip = p.rz < 22 ? Math.max(p.s, focal / 13) : p.s;
    var w = sChip * 0.62;
    var h = sChip * 0.3;
    if (p.rz < 20) {
      w = Math.max(w, 72);
      h = Math.max(h, 22);
    }
    ctx.save();
    ctx.translate(p.sx, p.sy);
    ctx.rotate(Math.sin(spin) * 0.35);
    ctx.globalCompositeOperation = "lighter";
    ctx.fillStyle = "rgba(255,209,102,0.22)";
    ctx.beginPath();
    ctx.ellipse(0, 0, w * 0.85, h * 0.95, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalCompositeOperation = "source-over";
    ctx.fillStyle = "rgba(18, 28, 32, 0.72)";
    roundRect(-w * 0.5, -h * 0.5, w, h, Math.max(3, p.s * 0.05));
    ctx.fill();
    ctx.strokeStyle = GOLD;
    ctx.lineWidth = Math.max(1.2, p.s * 0.03);
    ctx.stroke();
    ctx.fillStyle = GOLD;
    var label = String(meta.chip || o.id);
    fitFont(label, w * 0.88, Math.max(9, h * 0.48), 8);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(label, 0, 0);
    ctx.restore();
    var pole0 = project(x, 0.1, z);
    var pole1 = project(x, y - 0.12, z);
    if (pole0 && pole1) {
      ctx.strokeStyle = fogStyle("#8a6a30", p.rz, 0.45) || GOLD;
      ctx.lineWidth = Math.max(1, p.s * 0.02);
      ctx.beginPath();
      ctx.moveTo(pole0.sx, pole0.sy);
      ctx.lineTo(pole1.sx, pole1.sy);
      ctx.stroke();
    }
  }

  function drawSign(sign, z, stop) {
    var lane = sign.lane;
    if (lane !== 0 && lane !== 1 && lane !== 2) lane = 1;
    var x = laneX(lane);
    var y = 2.62;
    var p = project(x, y, z);
    if (!p) return;
    var rz = p.rz;
    if (rz > 62 || rz < 3.2) return;
    var sTrue = p.s;
    var spreadT = clamp((42 - rz) / 24, 0.4, 1);
    var gap = Math.min(W * 0.25, 210);
    var sx = cx + (lane - 1) * gap * spreadT;
    var sy = p.sy - Math.min(H * 0.07, 48) * spreadT;
    var w = Math.max(sTrue * 1.35, Math.min(W * 0.21, 176) * spreadT);
    var h = Math.max(sTrue * 0.4, Math.min(H * 0.058, 46) * spreadT);
    var text = String(sign.text || "");
    var col = laneColor(lane);
    var pole = project(laneX(lane), 0.1, z);
    if (pole) {
      ctx.strokeStyle = fogStyle("#3a414e", rz, 0.75) || METAL;
      ctx.lineWidth = Math.max(1.4, sTrue * 0.04);
      ctx.beginPath();
      ctx.moveTo(pole.sx, pole.sy);
      ctx.lineTo(sx, sy + h * 0.5);
      ctx.stroke();
    }
    ctx.save();
    ctx.translate(sx, sy);
    ctx.globalAlpha = clamp(1.05 - rz / 70, 0.4, 1);
    ctx.fillStyle = "rgba(4, 6, 10, 0.92)";
    roundRect(-w * 0.5, -h * 0.5, w, h, Math.max(4, h * 0.12));
    ctx.fill();
    ctx.lineWidth = Math.max(2, h * 0.06);
    ctx.strokeStyle = col;
    ctx.stroke();
    ctx.globalCompositeOperation = "lighter";
    ctx.strokeStyle = laneGlow(lane);
    ctx.lineWidth = 1.2;
    ctx.stroke();
    ctx.globalCompositeOperation = "source-over";
    ctx.fillStyle = "#f4fff8";
    var fs = fitFont(text, w * 0.88, Math.max(11, Math.min(22, h * 0.42)), 10);
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.shadowColor = col;
    ctx.shadowBlur = 8;
    ctx.fillText(text, 0, 1);
    ctx.shadowBlur = 0;
    ctx.fillStyle = col;
    ctx.font = "700 " + Math.max(8, fs * 0.48) + "px ui-monospace, monospace";
    ctx.fillText(laneKey(lane), 0, -h * 0.5 - Math.max(7, h * 0.2));
    ctx.restore();
  }

  function drawOriginRoom(stop, state, time) {
    var z = (stop.worldZ || 0) + (stop.length || 80) * 0.78;
    var rz = z - cam.z;
    var floor = fogStyle("#1a140c", rz, 0.85);
    quad(project(-3.4, -0.05, z - 10), project(3.4, -0.05, z - 10), project(3.4, -0.05, z + 6), project(-3.4, -0.05, z + 6), floor);
    quad(project(-3.6, -0.05, z + 5.5), project(3.6, -0.05, z + 5.5), project(3.6, 4.2, z + 5.5), project(-3.6, 4.2, z + 5.5), fogStyle("#120e0a", rz, 0.7));
    var i;
    for (i = -1; i <= 1; i += 2) {
      quad(
        project(i * 2.8, 0.1, z - 2),
        project(i * 3.3, 0.1, z - 2),
        project(i * 3.3, 2.4, z - 2),
        project(i * 2.8, 2.4, z - 2),
        fogStyle("#2a2418", rz, 0.55),
      );
    }
    var path = (state.packet && state.packet.path) || (stop.signs && stop.signs[0] && stop.signs[0].text) || "/";
    var sp = project(0, 2.55, z - 2);
    if (sp) {
      ctx.save();
      ctx.font = "700 " + Math.max(12, sp.s * 0.18) + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.fillStyle = GOLD;
      ctx.shadowColor = GOLD;
      ctx.shadowBlur = 16;
      ctx.fillText(String(path), sp.sx, sp.sy);
      ctx.restore();
    }
    if (state.ending === "origin404") drawGiant404(state, z - 1.5, time);
    ctx.save();
    ctx.globalCompositeOperation = "lighter";
    var g = project(0, 1.6, z + 3);
    if (g) {
      var grd = ctx.createRadialGradient(g.sx, g.sy, 0, g.sx, g.sy, Math.max(40, g.s * 3.5));
      grd.addColorStop(0, "rgba(255,209,102,0.35)");
      grd.addColorStop(1, "rgba(255,209,102,0)");
      ctx.fillStyle = grd;
      ctx.beginPath();
      ctx.arc(g.sx, g.sy, Math.max(40, g.s * 3.5), 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.restore();
  }

  function drawGiant404(state, z, time) {
    var p = project(0, 1.65, z);
    if (!p) return;
    var fs = Math.max(72, Math.min(H * 0.28, p.s * 2.35));
    var gap = fs * 0.62;
    var digits = ["4", "0", "4"];
    var i;
    ctx.save();
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.font = "800 " + fs + "px ui-monospace, SF Mono, Menlo, monospace";
    for (i = 0; i < 3; i++) {
      var dx = (i - 1) * gap + Math.sin(time * 3 + i) * 2;
      var dy = Math.sin(time * 2 + i * 1.3) * fs * 0.02;
      ctx.save();
      ctx.globalCompositeOperation = "lighter";
      ctx.fillStyle = "rgba(255,209,102,0.28)";
      ctx.fillText(digits[i], p.sx + dx, p.sy + dy);
      ctx.globalCompositeOperation = "source-over";
      ctx.lineWidth = Math.max(4, fs * 0.05);
      ctx.strokeStyle = "rgba(6,5,12,0.9)";
      ctx.strokeText(digits[i], p.sx + dx, p.sy + dy);
      ctx.fillStyle = GOLD;
      ctx.shadowColor = "rgba(255,209,102,0.85)";
      ctx.shadowBlur = 24;
      ctx.fillText(digits[i], p.sx + dx, p.sy + dy);
      ctx.restore();
    }
    ctx.restore();
  }

  function collectParticles(state) {
    var list = state.particles;
    var n = list.length > 80 ? 80 : list.length;
    var i;
    var q;
    for (i = 0; i < n; i++) {
      q = list[i];
      if (!q) continue;
      pushSprite(q.z || 0, 8, drawParticle.bind(null, q));
    }
  }

  function drawParticle(q) {
    var p = project(q.x || 0, q.y || 0, q.z || 0);
    if (!p) return;
    var life = q.life == null ? 1 : clamp(q.life, 0, 1);
    var r = Math.max(0.8, (q.size || 0.08) * p.s);
    ctx.globalAlpha = life * clamp(1 - p.rz / DRAW_DIST, 0, 1);
    ctx.fillStyle = q.color || PHOS;
    ctx.beginPath();
    ctx.rect(p.sx - r * 0.5, p.sy - r * 0.5, r, r);
    ctx.fill();
    ctx.globalAlpha = 1;
  }

  function roundRect(x, y, w, h, r) {
    var rr = Math.min(r, w * 0.5, h * 0.5);
    ctx.beginPath();
    ctx.moveTo(x + rr, y);
    ctx.arcTo(x + w, y, x + w, y + h, rr);
    ctx.arcTo(x + w, y + h, x, y + h, rr);
    ctx.arcTo(x, y + h, x, y, rr);
    ctx.arcTo(x, y, x + w, y, rr);
    ctx.closePath();
  }

  function drawPacket(state, time, reduced) {
    var pkt = state.packet;
    if (!pkt) return;
    if (!pkt.alive) {
      drawDeath(pkt, state, time);
      return;
    }
    var x = pkt.x;
    if (x == null) x = laneX(pkt.lane || 1);
    var y = pkt.y || 0;
    var z = pkt.z || 0;
    var h = pkt.h || (pkt.ducking ? 0.4 : 0.92);
    var ducking = !!pkt.ducking;
    var jumping = y > 0.12;
    var wBody = ducking ? 0.78 : 0.62;
    var lane = pkt.lane;
    if (lane !== 0 && lane !== 1 && lane !== 2) lane = 1;
    var stretch = jumping ? 1.08 : 1;
    var sh = project(x, 0.02, z);
    if (sh) {
      ctx.save();
      ctx.globalAlpha = jumping ? 0.18 : 0.32;
      ctx.fillStyle = "#000";
      ctx.beginPath();
      ctx.ellipse(sh.sx, sh.sy, sh.s * (0.26 + y * 0.04), sh.s * 0.09, 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }
    drawCape(pkt, x, y, z, h, time, reduced);
    var core = project(x, y + h * 0.46 * stretch, z);
    if (!core) return;
    var rx = core.s * wBody * 0.52;
    var ry = core.s * h * 0.48 * stretch;
    ctx.save();
    ctx.globalCompositeOperation = "lighter";
    ctx.fillStyle = laneGlow(lane);
    ctx.beginPath();
    ctx.ellipse(core.sx, core.sy, rx * 1.85, ry * 1.7, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "rgba(255,209,102,0.18)";
    ctx.beginPath();
    ctx.ellipse(core.sx, core.sy, rx * 2.4, ry * 2.1, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
    ctx.fillStyle = fogStyle("#1a2e28", core.rz, 1) || "#1a2e28";
    ctx.beginPath();
    ctx.ellipse(core.sx, core.sy, rx, ry, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = COPPER_HOT;
    ctx.lineWidth = Math.max(2.2, core.s * 0.07);
    ctx.stroke();
    ctx.strokeStyle = laneColor(lane);
    ctx.lineWidth = Math.max(1.2, core.s * 0.03);
    ctx.stroke();
    ctx.save();
    ctx.globalCompositeOperation = "lighter";
    ctx.fillStyle = "rgba(61,240,194,0.7)";
    ctx.beginPath();
    ctx.ellipse(core.sx, core.sy - ry * 0.12, rx * 0.55, ry * 0.48, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "rgba(244,255,248,0.85)";
    ctx.beginPath();
    ctx.ellipse(core.sx, core.sy - ry * 0.18, rx * 0.22, ry * 0.2, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
    var path = shortPath(pkt.path || "/");
    ctx.save();
    ctx.fillStyle = "#06201a";
    ctx.globalAlpha = 0.55;
    roundRect(core.sx - rx * 0.82, core.sy + ry * 0.12, rx * 1.64, ry * 0.42, ry * 0.12);
    ctx.fill();
    ctx.globalAlpha = 1;
    ctx.fillStyle = PHOS;
    ctx.font = "700 " + Math.max(9, core.s * 0.1) + "px ui-monospace, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(path, core.sx, core.sy + ry * 0.32);
    ctx.restore();
    if (state.ending === "origin404") {
      ctx.save();
      ctx.globalCompositeOperation = "lighter";
      ctx.fillStyle = "rgba(255,209,102,0.4)";
      ctx.beginPath();
      ctx.ellipse(core.sx, core.sy, rx * 2.2, ry * 1.9, 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }
  }

  function shortPath(p) {
    p = String(p || "/");
    if (p.length <= 22) return p;
    return p.slice(0, 10) + "…" + p.slice(-8);
  }

  function drawCape(pkt, x, y, z, h, time, reduced) {
    var phase = pkt.capePhase != null ? pkt.capePhase : time * 7;
    var baseY = y + h * 0.42;
    var i;
    var k;
    var segs = 5;
    var tags = pkt.tags || [];
    for (i = 0; i < 4; i++) {
      var pts = [];
      for (k = 0; k <= segs; k++) {
        var t = k / segs;
        var zz = z - 0.28 - t * 1.35;
        var wave = reduced ? 0 : Math.sin(phase + i * 0.9 + t * 3.1) * 0.05 * t;
        var xx = x + (i - 1.5) * (0.08 + t * 0.14) + wave;
        var yy = baseY - t * 0.08 + (reduced ? 0 : Math.sin(phase * 1.25 + i) * 0.03);
        var p = project(xx, yy, zz);
        if (p) pts.push(p);
      }
      if (pts.length < 2) continue;
      ctx.beginPath();
      ctx.moveTo(pts[0].sx, pts[0].sy);
      for (k = 1; k < pts.length; k++) ctx.lineTo(pts[k].sx, pts[k].sy);
      var col = i % 2 ? laneColor(pkt.lane || 1) : COPPER_HOT;
      ctx.strokeStyle = fogStyle(col, pts[0].rz, 0.7) || col;
      ctx.lineWidth = Math.max(2, pts[0].s * 0.055);
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      ctx.stroke();
    }
    var label = tags.length ? (N.PICKUP_META[tags[tags.length - 1]] || {}).chip : CAPE_BITS[0];
    var tip = project(x, baseY - 0.05, z - 0.9);
    if (tip && label) {
      ctx.fillStyle = fogStyle(PHOS, tip.rz, 0.75) || PHOS;
      ctx.font = "600 " + Math.max(8, tip.s * 0.09) + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.fillText(label, tip.sx, tip.sy);
    }
  }

  function armDeath(pkt) {
    shards.length = 0;
    var i;
    for (i = 0; i < 22; i++) {
      shards.push({
        x: (pkt.x || 0) + (hash01(i, 1, 9) - 0.5) * 0.45,
        y: (pkt.y || 0) + hash01(i, 2, 8) * (pkt.h || 0.9),
        z: (pkt.z || 0) + (hash01(i, 3, 7) - 0.5) * 0.55,
        vx: (hash01(i, 4, 6) - 0.5) * 3.4,
        vy: 1.8 + hash01(i, 5, 5) * 3.8,
        vz: (hash01(i, 6, 4) - 0.5) * 2.2,
        w: 0.07 + hash01(i, 7, 3) * 0.16,
        hh: 0.05 + hash01(i, 8, 2) * 0.14,
        color: i % 3 === 0 ? DANGER : i % 3 === 1 ? GOLD : FIBER,
      });
    }
    deathArmed = true;
  }

  function drawDeath(pkt, state, time) {
    if (!deathArmed) armDeath(pkt);
    var dt = 1 / 60;
    var i;
    var s;
    var p;
    for (i = 0; i < shards.length; i++) {
      s = shards[i];
      s.x += s.vx * dt;
      s.y += s.vy * dt;
      s.z += s.vz * dt;
      s.vy -= 9 * dt;
      p = project(s.x, s.y, s.z);
      if (!p) continue;
      ctx.fillStyle = fogStyle(s.color, p.rz, 0.95) || s.color;
      ctx.globalCompositeOperation = i % 2 ? "lighter" : "source-over";
      ctx.fillRect(p.sx, p.sy, Math.max(2, s.w * p.s), Math.max(2, s.hh * p.s));
      ctx.globalCompositeOperation = "source-over";
    }
  }

  function drawStreaks(speed, time) {
    var n = 16;
    var i;
    var seed;
    var sx;
    var sy;
    var vx;
    var vy;
    var len;
    var mag;
    ctx.save();
    ctx.globalCompositeOperation = "lighter";
    for (i = 0; i < n; i++) {
      seed = hash01((time * 48 + i) | 0, i, 3);
      sx = W * seed;
      sy = H * (0.22 + hash01(i, 9, 1) * 0.7);
      vx = sx - cx;
      vy = sy - cy;
      mag = Math.sqrt(vx * vx + vy * vy) || 1;
      len = 10 + (speed - 17) * 1.8 + hash01(i, 2, 4) * 22;
      ctx.strokeStyle = "rgba(212,255,154,0.2)";
      ctx.lineWidth = 1.15;
      ctx.beginPath();
      ctx.moveTo(sx, sy);
      ctx.lineTo(sx + (vx / mag) * len, sy + (vy / mag) * len);
      ctx.stroke();
    }
    ctx.restore();
  }

  function drawScanlines() {
    ctx.save();
    ctx.globalAlpha = 0.04;
    ctx.fillStyle = "#d4ff9a";
    var y;
    for (y = 0; y < H; y += 3) ctx.fillRect(0, y, W, 1);
    ctx.restore();
  }

  function clamp01(v) {
    return v < 0 ? 0 : v > 1 ? 1 : v;
  }

  function easeOutCubic(t) {
    t = clamp01(t);
    return 1 - Math.pow(1 - t, 3);
  }

  function easeOutBack(t) {
    t = clamp01(t);
    var c = 1.12;
    t = t - 1;
    return 1 + t * t * ((c + 1) * t + c);
  }

  function easeInOutCubic(t) {
    t = clamp01(t);
    return t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;
  }

  function quadBezier(a, b, c, t) {
    var u = 1 - t;
    return {
      x: u * u * a.x + 2 * u * t * b.x + t * t * c.x,
      y: u * u * a.y + 2 * u * t * b.y + t * t * c.y,
    };
  }

  function skinGrad(x0, y0, x1, y1) {
    var grd = ctx.createLinearGradient(x0, y0, x1, y1);
    grd.addColorStop(0, "#f7dfc8");
    grd.addColorStop(0.45, "#e4b394");
    grd.addColorStop(1, "#c48968");
    return grd;
  }

  function drawIntroFinger(x, y, ang, press, size, alpha) {
    var s = size;
    ctx.save();
    ctx.globalAlpha = alpha;
    ctx.translate(x, y);
    ctx.rotate(ang);
    ctx.fillStyle = "rgba(6,5,12," + (0.16 + press * 0.2) + ")";
    ctx.beginPath();
    ctx.ellipse(s * 0.02, s * 0.04 + press * s * 0.05, s * 0.18, s * 0.06, 0.2, 0, Math.PI * 2);
    ctx.fill();
    ctx.translate(0, press * s * 0.04);
    ctx.fillStyle = skinGrad(-s * 0.2, s * 0.9, s * 0.2, s * 1.4);
    ctx.beginPath();
    ctx.ellipse(-s * 0.02, s * 1.22, s * 0.28, s * 0.22, 0.35, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.ellipse(s * 0.16, s * 1.05, s * 0.07, s * 0.16, 0.45, 0, Math.PI * 2);
    ctx.ellipse(s * 0.26, s * 1.12, s * 0.06, s * 0.14, 0.55, 0, Math.PI * 2);
    ctx.fill();
    ctx.beginPath();
    ctx.moveTo(s * 0.012, -s * 0.055);
    ctx.bezierCurveTo(s * 0.1, -s * 0.08, s * 0.14, 0, s * 0.12, s * 0.14);
    ctx.lineTo(s * 0.16, s * 0.82);
    ctx.bezierCurveTo(s * 0.18, s * 1.08, -s * 0.02, s * 1.16, -s * 0.14, s * 1.02);
    ctx.lineTo(-s * 0.16, s * 0.7);
    ctx.bezierCurveTo(-s * 0.18, s * 0.26, -s * 0.12, -s * 0.02, s * 0.012, -s * 0.055);
    ctx.closePath();
    ctx.fillStyle = skinGrad(-s * 0.16, s * 0.02, s * 0.14, s * 0.55);
    ctx.fill();
    ctx.strokeStyle = "rgba(92, 52, 36, 0.22)";
    ctx.lineWidth = Math.max(1, s * 0.014);
    ctx.stroke();
    ctx.beginPath();
    ctx.ellipse(s * 0.018, -s * 0.006, s * 0.062, s * 0.078, -0.18, 0, Math.PI * 2);
    ctx.fillStyle = "#f8f1ea";
    ctx.fill();
    ctx.fillStyle = "rgba(255,255,255,0.45)";
    ctx.beginPath();
    ctx.ellipse(-s * 0.006, -s * 0.022, s * 0.024, s * 0.034, -0.22, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = "rgba(150, 90, 70, 0.18)";
    ctx.beginPath();
    ctx.moveTo(-s * 0.08, s * 0.38);
    ctx.quadraticCurveTo(s * 0.02, s * 0.35, s * 0.09, s * 0.42);
    ctx.stroke();
    ctx.restore();
  }

  function drawIntroCapsule(x, y, rw, rh, label, alpha) {
    ctx.save();
    ctx.globalAlpha = alpha;
    ctx.translate(x, y);
    ctx.fillStyle = "rgba(61,240,194,0.28)";
    ctx.beginPath();
    ctx.ellipse(0, 0, rw * 1.55, rh * 1.45, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "#15241f";
    ctx.beginPath();
    ctx.ellipse(0, 0, rw, rh, 0, 0, Math.PI * 2);
    ctx.fill();
    ctx.strokeStyle = "#e8a056";
    ctx.lineWidth = Math.max(1.5, rw * 0.08);
    ctx.stroke();
    ctx.fillStyle = "#3df0c2";
    ctx.beginPath();
    ctx.ellipse(0, -rh * 0.18, rw * 0.38, rh * 0.32, 0, 0, Math.PI * 2);
    ctx.fill();
    if (label) {
      ctx.fillStyle = "#d4ff9a";
      ctx.font = "700 " + Math.max(8, rw * 0.28) + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText(label, 0, rh * 0.42);
    }
    ctx.restore();
  }

  function drawIntro(state) {
    var intro = state.intro;
    if (!intro || !intro.on) return;
    var t = intro.t || 0;
    var dur = Math.max(intro.dur || 3.6, 0.01);
    var u = t / dur;
    var path = String(intro.path || "/");
    var host = String(intro.host || "lab");
    var short = path.length > 22 ? path.slice(0, 10) + "…" + path.slice(-8) : path;
    var phW = Math.min(W * 0.34, H * 0.24, 280);
    var phH = phW * 2.05;
    var enter = easeOutBack(clamp01(u / 0.18));
    var recede = easeInOutCubic(clamp01((u - 0.52) / 0.34));
    var phX = W * 0.5;
    var phY = lerp(H * 1.08, H * 0.46, enter) - recede * H * 0.2;
    var scale = lerp(0.84, 1, enter) * (1 - recede * 0.42);
    var veil = (1 - recede * 0.92) * clamp01(enter * 1.6);
    if (veil < 0.02) return;

    var w = phW * scale;
    var h = phH * scale;
    var screenL = phX - w * 0.4;
    var screenT = phY - h * 0.4;
    var screenW = w * 0.8;
    var screenH = h * 0.76;
    var btnX = phX;
    var btnY = screenT + screenH * 0.7;
    var btnW = w * 0.38;
    var btnH = h * 0.075;
    var getY = screenT + screenH * 0.46;
    var press = u > 0.32 && u < 0.46 ? Math.sin(clamp01((u - 0.32) / 0.1) * Math.PI) : 0;
    var peel = easeInOutCubic(clamp01((u - 0.4) / 0.14));
    var slide = easeInOutCubic(clamp01((u - 0.52) / 0.26));
    var from = { x: phX, y: getY };
    var chin = { x: phX, y: phY + h * 0.5 };
    var to = { x: W * 0.5, y: H * 0.78 };
    var seed;
    if (slide <= 0) {
      seed = from;
    } else if (slide < 0.32) {
      seed = quadBezier(from, { x: from.x, y: lerp(from.y, chin.y, 0.55) }, chin, slide / 0.32);
    } else {
      seed = quadBezier(chin, { x: lerp(chin.x, to.x, 0.45), y: lerp(chin.y, to.y, 0.35) }, to, (slide - 0.32) / 0.68);
    }

    ctx.save();
    ctx.globalAlpha = veil * 0.5;
    ctx.fillStyle = "#06050c";
    ctx.fillRect(0, 0, W, H);
    ctx.restore();

    ctx.save();
    ctx.globalAlpha = veil;
    ctx.translate(phX, phY);
    ctx.scale(scale, scale);
    ctx.fillStyle = "#14161e";
    roundRect(-phW * 0.5, -phH * 0.5, phW, phH, phW * 0.13);
    ctx.fill();
    ctx.strokeStyle = "rgba(61,240,194,0.55)";
    ctx.lineWidth = Math.max(2, phW * 0.016);
    ctx.stroke();
    ctx.fillStyle = "#08090e";
    roundRect(-phW * 0.42, -phH * 0.41, phW * 0.84, phH * 0.78, phW * 0.045);
    ctx.fill();
    ctx.fillStyle = "#1a1c24";
    roundRect(-phW * 0.07, phH * 0.385, phW * 0.14, phH * 0.036, 8);
    ctx.fill();

    var screenTop = -phH * 0.39;
    ctx.fillStyle = "#10141c";
    roundRect(-phW * 0.38, screenTop, phW * 0.76, phH * 0.07, 5);
    ctx.fill();
    ctx.fillStyle = "#3df0c2";
    ctx.font = "600 " + Math.max(9, phW * 0.042) + "px ui-monospace, monospace";
    ctx.textAlign = "left";
    ctx.textBaseline = "alphabetic";
    ctx.fillText("https://" + host, -phW * 0.34, screenTop + phH * 0.046);
    ctx.fillStyle = "#d7efe6";
    ctx.font = "800 " + Math.max(15, phW * 0.085) + "px ui-sans-serif, sans-serif";
    ctx.fillText("neo", -phW * 0.34, screenTop + phH * 0.16);
    ctx.fillStyle = "#7d9a93";
    ctx.font = "600 " + Math.max(8, phW * 0.038) + "px ui-monospace, monospace";
    ctx.fillText("homeserver", -phW * 0.34, screenTop + phH * 0.205);

    var cardA = 1 - peel;
    ctx.globalAlpha = veil * Math.max(0, cardA);
    ctx.fillStyle = "rgba(61,240,194,0.1)";
    roundRect(-phW * 0.34, screenTop + phH * 0.24, phW * 0.68, phH * 0.2, 9);
    ctx.fill();
    ctx.strokeStyle = "rgba(61,240,194," + (0.28 + peel * 0.5) + ")";
    ctx.lineWidth = 1.25;
    ctx.stroke();
    if (peel < 0.55) {
      ctx.fillStyle = "#ffd166";
      ctx.font = "700 " + Math.max(9, phW * 0.04) + "px ui-monospace, monospace";
      ctx.fillText("GET " + short, -phW * 0.28, screenTop + phH * 0.325);
      ctx.fillStyle = "#d7efe6";
      ctx.font = "600 " + Math.max(8, phW * 0.034) + "px ui-sans-serif, sans-serif";
      ctx.fillText("Open this path", -phW * 0.28, screenTop + phH * 0.39);
    }
    ctx.globalAlpha = veil;

    var localBtnY = screenTop + phH * 0.5;
    ctx.fillStyle = press > 0.15 ? "#2bb89a" : "#3df0c2";
    roundRect(-phW * 0.19, localBtnY + press * phH * 0.008, phW * 0.38, phH * 0.072, 999);
    ctx.fill();
    ctx.fillStyle = "#06201a";
    ctx.font = "800 " + Math.max(10, phW * 0.046) + "px ui-sans-serif, sans-serif";
    ctx.textAlign = "center";
    ctx.fillText("Open", 0, localBtnY + phH * 0.048 + press * phH * 0.008);
    ctx.restore();

    if (press > 0.02 && u < 0.55) {
      ctx.save();
      ctx.globalCompositeOperation = "lighter";
      ctx.globalAlpha = press * 0.45 * veil;
      ctx.strokeStyle = "#3df0c2";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(btnX, btnY + btnH * 0.5, btnW * (0.55 + (1 - press) * 0.7), 0, Math.PI * 2);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(btnX, btnY + btnH * 0.5, btnW * (0.35 + (1 - press) * 0.35), 0, Math.PI * 2);
      ctx.stroke();
      ctx.restore();
    }

    var fingerIn = easeOutCubic(clamp01((u - 0.14) / 0.18));
    var fingerOut = easeInOutCubic(clamp01((u - 0.42) / 0.08));
    if (fingerIn > 0 && fingerOut < 0.97) {
      var hover = {
        x: btnX + btnW * 0.1,
        y: btnY + btnH * 0.55,
      };
      var rest = { x: W * 0.76, y: H * 0.94 };
      var fx = lerp(rest.x, hover.x, fingerIn);
      var fy = lerp(rest.y, hover.y, fingerIn) + fingerOut * H * 0.2;
      drawIntroFinger(fx, fy, -0.78, press, Math.min(W, H) * 0.1, veil * (1 - fingerOut) * fingerIn);
    }

    if (peel > 0 && slide <= 0) {
      var stripW = lerp(screenW * 0.7, Math.min(W, H) * 0.08, peel);
      var stripH = lerp(screenH * 0.075, Math.min(W, H) * 0.03, peel);
      ctx.save();
      ctx.beginPath();
      ctx.rect(screenL, screenT, screenW, screenH);
      ctx.clip();
      ctx.globalAlpha = veil * (0.55 + peel * 0.45);
      ctx.fillStyle = "rgba(61,240,194,0.2)";
      roundRect(seed.x - stripW * 0.5, seed.y - stripH * 0.5, stripW, stripH, stripH * 0.45);
      ctx.fill();
      ctx.fillStyle = "#ffd166";
      ctx.font = "700 " + Math.max(8, stripH * 0.42) + "px ui-monospace, monospace";
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      ctx.fillText("GET " + short, seed.x, seed.y);
      ctx.restore();
    }

    if (slide > 0) {
      var capA = clamp01(slide * 4) * (1 - easeInOutCubic(clamp01((slide - 0.9) / 0.1)));
      var rw = Math.max(16, Math.min(W, H) * 0.042) * (0.62 + Math.min(slide, 0.8) * 0.5);
      if (slide > 0.32 && capA > 0.04) {
        ctx.save();
        ctx.globalCompositeOperation = "lighter";
        ctx.strokeStyle = "rgba(61,240,194," + (0.15 + capA * 0.35) + ")";
        ctx.lineWidth = Math.max(1.4, Math.min(W, H) * 0.005);
        ctx.beginPath();
        ctx.moveTo(chin.x, chin.y);
        ctx.quadraticCurveTo(lerp(chin.x, to.x, 0.45), lerp(chin.y, to.y, 0.35), seed.x, seed.y);
        ctx.stroke();
        ctx.restore();
      }
      if (capA > 0.04) {
        if (slide < 0.32) {
          ctx.save();
          ctx.beginPath();
          ctx.rect(screenL, screenT, screenW, screenH + h * 0.08);
          ctx.clip();
          drawIntroCapsule(seed.x, seed.y, rw, rw * 0.78, short, capA);
          ctx.restore();
        } else {
          drawIntroCapsule(seed.x, seed.y, rw, rw * 0.78, short, capA);
        }
      }
    }
  }

  function setupCamera(state, reduced) {
    var pkt = state.packet || {};
    var px = pkt.x;
    if (px == null) px = laneX(pkt.lane || 1);
    var py = pkt.y || 0;
    var pz = pkt.z || 0;
    var laneVel = pkt.laneVel || 0;
    if (!camReady || Math.abs(pz - lastPktZ) > 18) {
      camX = px * 0.55;
      camReady = true;
    }
    var dt = 1 / 60;
    if (state.time != null) {
      dt = clamp(state.time - lastTime, 0, 0.08);
      lastTime = state.time;
    }
    var follow = reduced ? 1 : 1 - Math.pow(0.08, Math.max(dt, 0.001) * 60);
    camX = lerp(camX, px * 0.55, follow);
    lastPktZ = pz;
    cam.x = camX;
    cam.roll = reduced ? 0 : -laneVel * 0.12;
    var shake = state.shake || {};
    var portrait = H > W * 1.18;
    if (portrait) {
      cam.y = 4.15 + py * 0.1;
      cam.z = pz - 3.45;
      cx = W * 0.5 + (shake.x || 0);
      cy = H * 0.16 + (shake.y || 0);
      focal = Math.max(0.5 * W, 0.42 * H);
    } else {
      cam.y = 2.48 + py * 0.18;
      cam.z = pz - 4.7;
      cx = W * 0.5 + (shake.x || 0);
      cy = H * 0.3 + (shake.y || 0);
      focal = 0.5 * Math.min(W, H);
    }
  }

  function draw(state) {
    if (!ctx || !el) return;
    state = state || {};
    resize();
    W = el.width;
    H = el.height;
    if (W < 2 || H < 2) return;
    var reduced = !!state.reducedMotion;
    var time = state.time || 0;
    var pkt = state.packet;
    if (pkt && pkt.alive) {
      deathArmed = false;
      shards.length = 0;
    }
    setupCamera(state, reduced);
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = VOID;
    ctx.fillRect(0, 0, W, H);
    ctx.save();
    if (cam.roll) {
      ctx.translate(W * 0.5, H * 0.5);
      ctx.rotate(cam.roll);
      ctx.translate(-W * 0.5, -H * 0.5);
    }
    var theme = (state.stop && (state.stop.theme || state.stop.id)) || "void";
    if (state.ending === "origin404") theme = "origin";
    drawBackdrop(theme);
    var stops = visibleStops(state);
    drawTunnel(stops, state, time);
    drawCables(stops, state, time);
    collectSprites(stops, state, time, reduced);
    spriteBuf.sort(function (a, b) {
      if (a.z !== b.z) return b.z - a.z;
      return a.layer - b.layer;
    });
    var i;
    for (i = 0; i < spriteBuf.length; i++) spriteBuf[i].draw();
    var intro = state.intro;
    var hidePkt = intro && intro.on && intro.t < (intro.dur || 3.6) * 0.78;
    if (!hidePkt) drawPacket(state, time, reduced);
    ctx.restore();
    if (intro && intro.on) drawIntro(state);
    if (!reduced && (state.speed || 0) > 17) drawStreaks(state.speed, time);
    drawScanlines();
    var flash = state.flash || 0;
    if (flash > 0) {
      ctx.fillStyle = rgba(255, 59, 107, clamp(flash, 0, 1) * 0.45);
      ctx.fillRect(0, 0, W, H);
    }
  }

  var api = {
    init: init,
    resize: resize,
    draw: draw,
  };

  root.Neo404 = Object.assign(root.Neo404 || {}, { Renderer: api });
  if (typeof module !== "undefined" && module.exports) module.exports = root.Neo404;
})(typeof globalThis !== "undefined" ? globalThis : this);
