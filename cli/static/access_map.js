// access_map.js
// Draws the lines of the access map (access.html.hbs, [data-neo-map]):
// where from → doors → app groups. Nodes are [data-neo-node="<id>"], lines are
// declared as [data-neo-edge] (data-from, data-to, data-tone, data-dashed) by
// routes/access_map.rs. Lines are only drawn where the columns stand side by
// side; on a phone the nodes say in words where they are reached from.
// Hovering a node highlights every path through it.
(function (root) {
  'use strict';

  var SVG_NS = 'http://www.w3.org/2000/svg';
  var TONES = {
    ok: 'text-success',
    info: 'text-info',
    warning: 'text-warning',
    error: 'text-error',
    muted: 'text-base-content/30',
  };

  /** y of anchor i of n on a side of `rect` (spread over the middle 60%). */
  function anchorY(rect, i, n) {
    var span = rect.height * 0.6;
    var top = rect.top + rect.height * 0.2;
    return n <= 1 ? rect.top + rect.height / 2 : top + (span * i) / (n - 1);
  }

  /** Horizontal S-curve from (x1, y1) to (x2, y2). */
  function curve(x1, y1, x2, y2) {
    var dx = Math.max(24, (x2 - x1) * 0.5);
    return 'M' + x1 + ' ' + y1 + ' C' + (x1 + dx) + ' ' + y1 + ' ' + (x2 - dx) + ' ' + y2 + ' ' + x2 + ' ' + y2;
  }

  /** Edge indices on every path through `id` (upstream and downstream). */
  function pathsThrough(edges, id) {
    var out = new Set();
    function walk(node, down) {
      edges.forEach(function (e, i) {
        if (out.has(i)) return;
        if (down ? e.from === node : e.to === node) {
          out.add(i);
          walk(down ? e.to : e.from, down);
        }
      });
    }
    walk(id, true);
    walk(id, false);
    return out;
  }

  function rel(el, base) {
    var r = el.getBoundingClientRect();
    return { left: r.left - base.left, right: r.right - base.left, top: r.top - base.top, height: r.height };
  }

  function draw(map) {
    var svg = map.querySelector('[data-neo-map-canvas]');
    if (!svg) return;
    while (svg.firstChild) svg.removeChild(svg.firstChild);
    var base = map.getBoundingClientRect();
    svg.setAttribute('viewBox', '0 0 ' + base.width + ' ' + base.height);
    var edges = map._neoEdges;
    var node = function (id) {
      return map.querySelector('[data-neo-node="' + id + '"]');
    };
    var rects = {};
    edges.forEach(function (e) {
      [e.from, e.to].forEach(function (id) {
        var el = node(id);
        if (el && !rects[id]) rects[id] = rel(el, base);
      });
    });
    // Anchors per side, sorted by the other end so lines do not cross.
    var outs = {};
    var ins = {};
    var drawn = edges.filter(function (e) {
      var a = rects[e.from];
      var b = rects[e.to];
      return a && b && b.left > a.right + 12;
    });
    drawn.forEach(function (e) {
      (outs[e.from] = outs[e.from] || []).push(e);
      (ins[e.to] = ins[e.to] || []).push(e);
    });
    var mid = function (id) {
      return rects[id].top + rects[id].height / 2;
    };
    Object.keys(outs).forEach(function (k) {
      outs[k].sort(function (a, b) { return mid(a.to) - mid(b.to); });
    });
    Object.keys(ins).forEach(function (k) {
      ins[k].sort(function (a, b) { return mid(a.from) - mid(b.from); });
    });
    var still = root.matchMedia && root.matchMedia('(prefers-reduced-motion: reduce)').matches;
    drawn.forEach(function (e) {
      var a = rects[e.from];
      var b = rects[e.to];
      var y1 = anchorY(a, outs[e.from].indexOf(e), outs[e.from].length);
      var y2 = anchorY(b, ins[e.to].indexOf(e), ins[e.to].length);
      var p = document.createElementNS(SVG_NS, 'path');
      p.setAttribute('d', curve(a.right, y1, b.left, y2));
      p.setAttribute('fill', 'none');
      p.setAttribute('stroke', 'currentColor');
      p.setAttribute('stroke-width', e.tone === 'error' ? '2.5' : '2');
      p.setAttribute('stroke-linecap', 'round');
      p.setAttribute('class', (TONES[e.tone] || TONES.muted) + ' transition-opacity duration-150');
      if (e.dashed || e.tone === 'error') p.setAttribute('stroke-dasharray', e.tone === 'error' ? '7 5' : '5 4');
      if (e.tone === 'error' && !still) {
        // Traffic flowing in: an open entry moves.
        var an = document.createElementNS(SVG_NS, 'animate');
        an.setAttribute('attributeName', 'stroke-dashoffset');
        an.setAttribute('from', '24');
        an.setAttribute('to', '0');
        an.setAttribute('dur', '0.9s');
        an.setAttribute('repeatCount', 'indefinite');
        p.appendChild(an);
      }
      p._neoEdge = e;
      svg.appendChild(p);
    });
    // End dots where a line meets a node.
    drawn.forEach(function (e) {
      var b = rects[e.to];
      var c = document.createElementNS(SVG_NS, 'circle');
      c.setAttribute('cx', b.left);
      c.setAttribute('cy', anchorY(b, ins[e.to].indexOf(e), ins[e.to].length));
      c.setAttribute('r', '3');
      c.setAttribute('fill', 'currentColor');
      c.setAttribute('class', TONES[e.tone] || TONES.muted);
      c._neoEdge = e;
      svg.appendChild(c);
    });
  }

  /** Hover shows a node's paths; a click pins them until the next click. */
  function highlight(map, id) {
    id = id || map._neoPinned || null;
    var edges = map._neoEdges;
    var on = id ? pathsThrough(edges, id) : null;
    var nodes = new Set();
    if (on) {
      nodes.add(id);
      on.forEach(function (i) {
        nodes.add(edges[i].from);
        nodes.add(edges[i].to);
      });
    }
    map.querySelectorAll('[data-neo-map-canvas] > *').forEach(function (el) {
      var i = edges.indexOf(el._neoEdge);
      el.classList.toggle('opacity-15', !!on && !on.has(i));
    });
    map.querySelectorAll('[data-neo-node]').forEach(function (el) {
      el.classList.toggle('opacity-40', !!on && !nodes.has(el.getAttribute('data-neo-node')));
    });
  }

  /** Read the edges, draw, and redraw when the map changes size. */
  function mount(map) {
    if (!map || map._neoMounted) return;
    map._neoMounted = true;
    map._neoEdges = Array.prototype.map.call(map.querySelectorAll('[data-neo-edge]'), function (el) {
      return {
        from: el.getAttribute('data-from'),
        to: el.getAttribute('data-to'),
        tone: el.getAttribute('data-tone') || 'muted',
        dashed: el.getAttribute('data-dashed') === 'true',
      };
    });
    var queued = false;
    var redraw = function () {
      if (queued) return;
      queued = true;
      root.requestAnimationFrame(function () {
        queued = false;
        draw(map);
      });
    };
    if (root.ResizeObserver) new root.ResizeObserver(redraw).observe(map);
    map.addEventListener('load', redraw, true);
    map.querySelectorAll('[data-neo-node]').forEach(function (el) {
      el.addEventListener('click', function () {
        var id = el.getAttribute('data-neo-node');
        map._neoPinned = map._neoPinned === id ? null : id;
        highlight(map, null);
      });
      el.addEventListener('mouseenter', function () { highlight(map, el.getAttribute('data-neo-node')); });
      el.addEventListener('mouseleave', function () { highlight(map, null); });
      el.addEventListener('focusin', function () { highlight(map, el.getAttribute('data-neo-node')); });
      el.addEventListener('focusout', function () { highlight(map, null); });
    });
    redraw();
  }

  var NeoAccessMap = { anchorY: anchorY, curve: curve, pathsThrough: pathsThrough, mount: mount };
  root.NeoAccessMap = NeoAccessMap;
  if (typeof module === 'object' && module.exports) {
    module.exports = NeoAccessMap;
  }
})(typeof globalThis !== 'undefined' ? globalThis : this);
