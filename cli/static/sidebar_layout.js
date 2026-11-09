// Navigator sidebar layout: drag & drop order and groups (folders), per browser.
//
// The server renders the services as a flat list of `.svc-btn` links in
// #nav-services; this module re-arranges those same elements (never clones
// them, so status dots / warm / active classes keep working) after the layout
// saved in localStorage. Nothing is stored on the server.
//
// Layout: a list of nodes, each a service name (string) or a group
// { id: "group:…", name, open, items: [service names] }. Groups do not nest.
// Services missing from the layout are appended in server order; entries for
// services that are gone stay stored (a re-enabled service gets its spot back)
// but are not rendered.
//
// Gestures: mouse drags after a few pixels, touch after a long press (so the
// sidebar still scrolls). Dropping on the middle of a service makes a group,
// on a group adds to it, near an edge reorders.
(function (root, factory) {
  var api = factory((root && root.NeoUtil) || require('./neo_util.js'));
  if (typeof module === 'object' && module.exports) module.exports = api;
  if (root && root.document) {
    root.NeoSidebarLayout = api;
    api.autostart(root);
  }
})(typeof window !== 'undefined' ? window : null, function (NeoUtil) {
  'use strict';

  var STORAGE_KEY = 'neo-sidebar-layout-v1';
  var GROUP_PREFIX = 'group:';

  // --- Model (pure; covered by sidebar_layout.test.js) ---

  function isGroup(node) {
    return node !== null && typeof node === 'object';
  }

  function nodeId(node) {
    return isGroup(node) ? node.id : node;
  }

  function newGroupId() {
    return GROUP_PREFIX + Date.now().toString(36) + Math.random().toString(36).slice(2, 6);
  }

  /** Drop malformed entries and duplicate services from a stored layout. */
  function sanitize(raw) {
    var seen = {};
    var out = [];
    if (!Array.isArray(raw)) return out;
    raw.forEach(function (node) {
      if (typeof node === 'string') {
        if (node && !seen[node]) { seen[node] = true; out.push(node); }
      } else if (isGroup(node) && typeof node.id === 'string' && node.id.indexOf(GROUP_PREFIX) === 0) {
        var items = [];
        (Array.isArray(node.items) ? node.items : []).forEach(function (n) {
          if (typeof n === 'string' && n && !seen[n]) { seen[n] = true; items.push(n); }
        });
        if (items.length) {
          out.push({
            id: node.id,
            name: typeof node.name === 'string' && node.name.trim() ? node.name : 'Group',
            open: node.open === true,
            items: items,
          });
        }
      }
    });
    return out;
  }

  /** Sanitized layout plus every service in `names` it does not mention yet. */
  function withServices(raw, names) {
    var tree = sanitize(raw);
    var seen = {};
    tree.forEach(function (n) {
      if (isGroup(n)) n.items.forEach(function (i) { seen[i] = true; });
      else seen[n] = true;
    });
    names.forEach(function (name) {
      if (!seen[name]) { seen[name] = true; tree.push(name); }
    });
    return tree;
  }

  /** { list, index, node, group } for a node id, or null. */
  function find(tree, id) {
    for (var i = 0; i < tree.length; i++) {
      var n = tree[i];
      if (nodeId(n) === id) return { list: tree, index: i, node: n, group: null };
      if (isGroup(n)) {
        for (var j = 0; j < n.items.length; j++) {
          if (n.items[j] === id) return { list: n.items, index: j, node: n.items[j], group: n };
        }
      }
    }
    return null;
  }

  function copy(tree) {
    return JSON.parse(JSON.stringify(tree));
  }

  function prune(tree) {
    return tree.filter(function (n) { return !isGroup(n) || n.items.length > 0; });
  }

  /**
   * Move node `dragId` to `target`:
   *   { type: 'before' | 'after', id }  next to a node (a group dropped next to
   *                                     a grouped service lands next to its group)
   *   { type: 'merge', id }             onto a service (new group) or group (append)
   *   { type: 'append', group }         end of a group, or of the top level (group null)
   * Returns a new tree; the input is not modified.
   */
  function move(tree, dragId, target) {
    if (!target || target.id === dragId) return tree;
    var t = copy(tree);
    var from = find(t, dragId);
    if (!from) return tree;
    var node = from.list.splice(from.index, 1)[0];
    var dragGroup = isGroup(node);
    var to;

    if (target.type === 'append') {
      var g = target.group && find(t, target.group);
      if (g && isGroup(g.node) && !dragGroup) g.node.items.push(node);
      else t.push(node);
      return prune(t);
    }

    to = find(t, target.id);
    if (!to) return tree;

    if (target.type === 'merge' && !dragGroup) {
      if (isGroup(to.node)) {
        to.node.items.push(node);
      } else if (to.group) {
        to.list.splice(to.index + 1, 0, node);
      } else {
        to.list.splice(to.index, 1, {
          id: newGroupId(),
          name: 'Group',
          open: true,
          items: [to.node, node],
        });
      }
      return prune(t);
    }

    if (dragGroup && to.group) to = find(t, to.group.id);
    var after = target.type === 'after';
    to.list.splice(to.index + (after ? 1 : 0), 0, node);
    return prune(t);
  }

  function updateGroup(tree, id, fn) {
    var t = copy(tree);
    var loc = find(t, id);
    if (loc && isGroup(loc.node)) fn(loc.node);
    return t;
  }

  function setOpen(tree, id, open) {
    return updateGroup(tree, id, function (g) { g.open = !!open; });
  }

  function rename(tree, id, name) {
    var clean = String(name || '').trim().slice(0, 40);
    return updateGroup(tree, id, function (g) { g.name = clean || 'Group'; });
  }

  /** Replace a group with its services, in place. */
  function ungroup(tree, id) {
    var t = copy(tree);
    var loc = find(t, id);
    if (!loc || !isGroup(loc.node)) return t;
    Array.prototype.splice.apply(t, [loc.index, 1].concat(loc.node.items));
    return t;
  }

  var model = {
    sanitize: sanitize,
    withServices: withServices,
    find: find,
    move: move,
    setOpen: setOpen,
    rename: rename,
    ungroup: ungroup,
    isGroup: isGroup,
    nodeId: nodeId,
  };

  // --- Storage (best effort: private windows may throw) ---

  var storage = NeoUtil.store('local');

  function load() {
    return storage.getJson(STORAGE_KEY, null);
  }

  function save(tree) {
    storage.setJson(STORAGE_KEY, tree);
  }

  function clear() {
    storage.remove(STORAGE_KEY);
  }

  // --- DOM ---

  var win = null;
  var doc = null;
  var tree = [];

  function container() {
    return doc.getElementById('nav-services');
  }

  function serviceEls() {
    var map = {};
    var order = [];
    var c = container();
    if (!c) return { map: map, order: order };
    c.querySelectorAll('.svc-btn[data-name]').forEach(function (el) {
      var name = el.getAttribute('data-name');
      if (map[name]) return;
      map[name] = el;
      order.push(name);
      el.setAttribute('data-node-id', name);
      el.setAttribute('draggable', 'false');
    });
    return { map: map, order: order };
  }

  function el(tag, cls, attrs) {
    var e = doc.createElement(tag);
    if (cls) e.className = cls;
    if (attrs) Object.keys(attrs).forEach(function (k) { e.setAttribute(k, attrs[k]); });
    return e;
  }

  /** Small preview of a service for the folder tile: its icon, else its initials. */
  function miniIcon(svc) {
    var cell = el('span', 'neo-group-mini');
    var img = svc.querySelector('img');
    if (img && img.getAttribute('src') && img.style.display !== 'none') {
      var m = el('img', '', { src: img.getAttribute('src'), alt: '', draggable: 'false' });
      cell.appendChild(m);
    } else {
      var badge = svc.querySelector('.font-mono');
      cell.textContent = badge ? badge.textContent.trim().slice(0, 2) : '•';
    }
    return cell;
  }

  function groupEl(group, svcs) {
    var present = group.items.filter(function (n) { return svcs[n]; });
    var wrap = el('div', 'neo-group', { 'data-group-id': group.id, 'data-open': group.open ? 'true' : 'false' });
    var tile = el('button', 'nav-btn neo-group-tile w-12 h-11 rounded-xl flex-shrink-0 flex items-center justify-center', {
      type: 'button',
      'data-node-id': group.id,
      title: group.name + ' (' + present.length + ')',
      'aria-expanded': group.open ? 'true' : 'false',
      'aria-label': group.name,
    });
    var grid = el('span', 'neo-group-grid');
    present.slice(0, 4).forEach(function (n) { grid.appendChild(miniIcon(svcs[n])); });
    tile.appendChild(grid);
    wrap.appendChild(tile);

    var items = el('div', 'neo-group-items');
    var label = el('button', 'neo-group-label', { type: 'button', title: 'Rename or ungroup "' + group.name + '"' });
    label.textContent = group.name;
    items.appendChild(label);
    present.forEach(function (n) { items.appendChild(svcs[n]); });
    wrap.appendChild(items);
    return wrap;
  }

  function render() {
    var c = container();
    if (!c) return;
    var found = serviceEls();
    if (!found.order.length) return;
    tree = withServices(load() || [], found.order);

    var frag = doc.createDocumentFragment();
    tree.forEach(function (node) {
      if (isGroup(node)) {
        if (node.items.some(function (n) { return found.map[n]; })) frag.appendChild(groupEl(node, found.map));
      } else if (found.map[node]) {
        frag.appendChild(found.map[node]);
      }
    });
    c.querySelectorAll('.neo-group').forEach(function (g) { g.remove(); });
    c.insertBefore(frag, c.firstChild);

    var reset = doc.getElementById('sidebar-reset');
    if (reset) reset.classList.toggle('hidden', load() === null);
  }

  function commit(next) {
    tree = next;
    save(tree);
    render();
  }

  // --- Group dialog (rename / ungroup) and reset ---

  function openGroupDialog(id) {
    var loc = find(tree, id);
    var dlg = doc.getElementById('sidebar-group-dialog');
    if (!loc || !isGroup(loc.node) || !dlg) return;
    var input = dlg.querySelector('input[name="name"]');
    dlg.setAttribute('data-group-id', id);
    dlg.returnValue = ''; // Escape keeps the previous value otherwise
    input.value = loc.node.name;
    dlg.showModal();
    input.focus();
    input.select();
  }

  function bindDialogs() {
    var dlg = doc.getElementById('sidebar-group-dialog');
    if (dlg) {
      // Enter would submit with the first button (Ungroup).
      dlg.querySelector('input[name="name"]').addEventListener('keydown', function (e) {
        if (e.key !== 'Enter') return;
        e.preventDefault();
        dlg.close('save');
      });
      dlg.addEventListener('close', function () {
        var id = dlg.getAttribute('data-group-id');
        var input = dlg.querySelector('input[name="name"]');
        if (!id) return;
        if (dlg.returnValue === 'save') commit(rename(tree, id, input.value));
        else if (dlg.returnValue === 'ungroup') commit(ungroup(tree, id));
        dlg.removeAttribute('data-group-id');
      });
    }
    var resetDlg = doc.getElementById('sidebar-reset-dialog');
    var resetBtn = doc.getElementById('sidebar-reset');
    if (resetDlg && resetBtn) {
      resetBtn.addEventListener('click', function () {
        resetDlg.returnValue = '';
        resetDlg.showModal();
      });
      resetDlg.addEventListener('close', function () {
        if (resetDlg.returnValue !== 'reset') return;
        clear();
        // The server order is the original DOM order: reload the fragment.
        if (win.htmx) win.htmx.ajax('GET', '/nav-services', { target: '#nav-services', swap: 'innerHTML' });
        else win.location.reload();
      });
    }
  }

  // --- Drag & drop ---

  var MOUSE_SLOP = 5;      // px before a mouse press becomes a drag
  var TOUCH_SLOP = 8;      // px of finger movement that cancels a pending long press
  var LONG_PRESS_MS = 350;

  var pending = null;      // { el, id, x, y, timer, touch }
  var drag = null;         // { el, id, group, ghost, dx, dy, x, y, target, scroll }
  var suppressClickUntil = 0;
  var indicator = null;

  function sidebar() {
    return doc.getElementById('sidebar');
  }

  function dragSource(target) {
    var t = target && target.closest && target.closest('[data-node-id]');
    return t && container() && container().contains(t) ? t : null;
  }

  function begin(x, y) {
    var src = pending.el;
    var id = src.getAttribute('data-node-id');
    var isGroupTile = src.classList.contains('neo-group-tile');
    var r = src.getBoundingClientRect();
    var ghost = src.cloneNode(true);
    ghost.removeAttribute('id');
    ghost.classList.add('neo-drag-ghost');
    ghost.style.width = r.width + 'px';
    ghost.style.height = r.height + 'px';
    doc.body.appendChild(ghost);

    var moving = isGroupTile ? src.closest('.neo-group') : src;
    moving.classList.add('neo-dragging');
    drag = { el: moving, id: id, group: isGroupTile, ghost: ghost, dx: x - r.left, dy: y - r.top, x: x, y: y, target: null, scroll: 0 };
    doc.body.classList.add('neo-sidebar-dragging');
    if (pending.touch && win.navigator.vibrate) {
      try { win.navigator.vibrate(10); } catch (_) {}
    }
    pending = null;
    position(x, y);
    autoscroll();
  }

  function clearTarget() {
    var c = container();
    if (c) c.querySelectorAll('.neo-drop-merge').forEach(function (e) { e.classList.remove('neo-drop-merge'); });
    if (indicator) indicator.style.display = 'none';
  }

  function showLine(rect, edge) {
    if (!indicator) {
      indicator = el('div', 'neo-drop-line');
      doc.body.appendChild(indicator);
    }
    indicator.style.display = 'block';
    indicator.style.left = rect.left + 4 + 'px';
    indicator.style.width = rect.width - 8 + 'px';
    indicator.style.top = (edge === 'before' ? rect.top - 2 : rect.bottom + 1) + 'px';
  }

  /** Drop target under the pointer, also painting the hint. */
  function hitTest(x, y) {
    clearTarget();
    var sb = sidebar();
    var hit = doc.elementFromPoint(x, y);
    if (!sb || !hit || !sb.contains(hit) || drag.el.contains(hit)) return null;

    var tile = hit.closest('[data-node-id]');
    if (tile && container().contains(tile)) {
      var id = tile.getAttribute('data-node-id');
      var r = tile.getBoundingClientRect();
      var f = (y - r.top) / r.height;
      var inGroup = !!tile.closest('.neo-group-items');
      var groupTile = tile.classList.contains('neo-group-tile');
      var box = groupTile ? tile.closest('.neo-group').getBoundingClientRect() : r;
      if (drag.group && inGroup) {
        // Groups do not nest: aim at the group this service lives in.
        var wrap = tile.closest('.neo-group');
        var wr = wrap.getBoundingClientRect();
        var before = y < wr.top + wr.height / 2;
        showLine(wr, before ? 'before' : 'after');
        return { type: before ? 'before' : 'after', id: wrap.getAttribute('data-group-id') };
      }
      if (!drag.group && f > 0.25 && (groupTile || (f < 0.75 && !inGroup))) {
        tile.classList.add('neo-drop-merge');
        return { type: 'merge', id: id };
      }
      var edge = f < 0.5 ? 'before' : 'after';
      showLine(edge === 'before' ? r : box, edge);
      return { type: edge, id: id };
    }

    var items = hit.closest('.neo-group-items');
    if (items && !drag.group) {
      var last = items.lastElementChild;
      if (last) showLine(last.getBoundingClientRect(), 'after');
      return { type: 'append', group: items.closest('.neo-group').getAttribute('data-group-id') };
    }

    var lastTop = container().lastElementChild;
    while (lastTop && !lastTop.matches('.svc-btn, .neo-group')) lastTop = lastTop.previousElementSibling;
    if (lastTop) {
      var lr = lastTop.getBoundingClientRect();
      if (y > lr.bottom) {
        showLine(lr, 'after');
        return { type: 'append', group: null };
      }
    }
    return null;
  }

  function position(x, y) {
    drag.x = x;
    drag.y = y;
    drag.ghost.style.transform = 'translate(' + (x - drag.dx) + 'px,' + (y - drag.dy) + 'px) scale(1.08)';
    drag.target = hitTest(x, y);
    var sb = sidebar();
    if (!sb) return;
    var r = sb.getBoundingClientRect();
    var edge = 40;
    drag.scroll = y < r.top + edge ? -6 : y > r.bottom - edge ? 6 : 0;
  }

  function autoscroll() {
    if (!drag) return;
    if (drag.scroll) {
      var sb = sidebar();
      if (sb) {
        sb.scrollTop += drag.scroll;
        drag.target = hitTest(drag.x, drag.y);
      }
    }
    win.requestAnimationFrame(autoscroll);
  }

  function finish(drop) {
    if (pending) {
      clearTimeout(pending.timer);
      pending = null;
    }
    if (!drag) return;
    var d = drag;
    drag = null;
    clearTarget();
    d.ghost.remove();
    d.el.classList.remove('neo-dragging');
    doc.body.classList.remove('neo-sidebar-dragging');
    suppressClickUntil = Date.now() + 400;
    if (drop && d.target) commit(move(tree, d.id, d.target));
  }

  function press(target, x, y, touch) {
    finish(false);
    var src = dragSource(target);
    if (!src) return;
    pending = { el: src, x: x, y: y, touch: touch, timer: null };
    if (touch) {
      pending.timer = setTimeout(function () {
        if (pending) begin(pending.x, pending.y);
      }, LONG_PRESS_MS);
    }
  }

  function moveTo(x, y) {
    if (drag) {
      position(x, y);
      return true;
    }
    if (!pending) return false;
    var dist = Math.abs(x - pending.x) + Math.abs(y - pending.y);
    if (pending.touch) {
      if (dist > TOUCH_SLOP) finish(false); // a scroll, not a long press
    } else if (dist > MOUSE_SLOP) {
      begin(x, y);
      return true;
    }
    return false;
  }

  function bindDrag() {
    var sb = sidebar();
    if (!sb) return;

    sb.addEventListener('pointerdown', function (e) {
      if (e.pointerType === 'touch' || e.button !== 0) return;
      if (e.ctrlKey || e.metaKey || e.shiftKey || e.altKey) return;
      press(e.target, e.clientX, e.clientY, false);
    });
    doc.addEventListener('pointermove', function (e) {
      if (e.pointerType === 'touch') return;
      if (moveTo(e.clientX, e.clientY)) e.preventDefault();
    });
    doc.addEventListener('pointerup', function (e) {
      if (e.pointerType !== 'touch') finish(true);
    });
    doc.addEventListener('pointercancel', function (e) {
      if (e.pointerType !== 'touch') finish(false);
    });

    // Touch uses touch events: preventDefault on a non-passive touchmove is the
    // one way to stop the sidebar scrolling under a drag on iOS Safari.
    sb.addEventListener('touchstart', function (e) {
      if (e.touches.length !== 1) { finish(false); return; }
      var t = e.touches[0];
      press(e.target, t.clientX, t.clientY, true);
    }, { passive: true });
    doc.addEventListener('touchmove', function (e) {
      if (!pending && !drag) return;
      var t = e.touches[0];
      if (t && moveTo(t.clientX, t.clientY) && e.cancelable) e.preventDefault();
    }, { passive: false });
    doc.addEventListener('touchend', function (e) {
      var wasDrag = !!drag;
      finish(true);
      if (wasDrag && e.cancelable) e.preventDefault(); // no click after a drop
    }, { passive: false });
    doc.addEventListener('touchcancel', function () { finish(false); });

    // A long press must not open the link callout / Android context menu (or
    // the navigator's right-click unload) while it is turning into a drag.
    doc.addEventListener('contextmenu', function (e) {
      if (pending && pending.touch || drag) {
        e.preventDefault();
        e.stopPropagation();
      }
    }, true);

    // Native link / image dragging would fight ours.
    sb.addEventListener('dragstart', function (e) {
      if (dragSource(e.target)) e.preventDefault();
    });

    // Swallow the click that ends a drag; handle group tiles and labels.
    sb.addEventListener('click', function (e) {
      if (Date.now() < suppressClickUntil) {
        e.preventDefault();
        e.stopPropagation();
        return;
      }
      var tile = e.target.closest('.neo-group-tile');
      if (tile) {
        var id = tile.getAttribute('data-node-id');
        var loc = find(tree, id);
        if (loc && isGroup(loc.node)) commit(setOpen(tree, id, !loc.node.open));
        return;
      }
      var label = e.target.closest('.neo-group-label');
      if (label) openGroupDialog(label.closest('.neo-group').getAttribute('data-group-id'));
    }, true);

    sb.addEventListener('contextmenu', function (e) {
      var tile = e.target.closest('.neo-group-tile');
      if (!tile) return;
      e.preventDefault();
      openGroupDialog(tile.getAttribute('data-node-id'));
    });

    doc.addEventListener('keydown', function (e) {
      if (e.key === 'Escape' && (drag || pending)) finish(false);
    });
  }

  function autostart(w) {
    win = w;
    doc = w.document;
    var start = function () {
      bindDrag();
      bindDialogs();
      render();
      doc.body.addEventListener('htmx:afterSwap', function (evt) {
        var t = evt.detail && evt.detail.target;
        if (t && t.id === 'nav-services') render();
      });
    };
    NeoUtil.onReady(doc, start);
  }

  return { model: model, autostart: autostart, render: render, STORAGE_KEY: STORAGE_KEY };
});
