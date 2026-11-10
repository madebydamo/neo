// agent_chat_refs.js — the `@` reference popup of the assistant chat input.
(function (root) {
  'use strict';

  var core = root.NeoAgentCore;
  var h = root.NeoUtil.h;

  /**
   * The `@` popup on the chat input: opens while an `@query` is typed, filters
   * fuzzily, ↑/↓ choose, Enter/Tab insert, Esc closes, click inserts.
   * `loadRefs()` returns a promise of the candidates; `key(e)` returns true when it
   * handled the key.
   */
  function picker(input, pop, loadRefs) {
    var st = { open: false, q: null, list: [], sel: 0, seq: 0 };
    if (!pop) return { key: function () { return false; }, close: function () {} };

    function close() {
      st.open = false;
      st.list = [];
      pop.classList.add('hidden');
      input.setAttribute('aria-expanded', 'false');
      input.removeAttribute('aria-activedescendant');
    }

    function row(it, i) {
      var hint = [];
      if (it.hint) hint.push(it.hint);
      if (it.kind === 'service' && !it.enabled) hint.push('not installed');
      if (it.secret) hint.push('secret, value stays hidden');
      var fallback = it.kind === 'option' ? '⚙' : it.kind === 'guide' ? '☰' : it.kind === 'privacy' ? '◎' : it.kind === 'finding' ? '!' : '▣';
      return h(
        'div',
        {
          id: 'neo-agent-ref-' + i,
          class: 'neo-agent-ref-opt' + (i === st.sel ? ' is-active' : ''),
          attrs: { role: 'option', 'aria-selected': i === st.sel ? 'true' : 'false' },
          on: {
            mousedown: function (e) {
              e.preventDefault(); // keep the focus in the input
              pick(i);
            },
            mousemove: function () {
              if (st.sel !== i) { st.sel = i; paint(); }
            },
          },
        },
        h('span', it.icon ? { class: 'neo-agent-ref-ico' } : { class: 'neo-agent-ref-ico', text: fallback },
          it.icon && h('img', { src: it.icon, alt: '' })),
        h('span', { class: 'min-w-0 flex-1' },
          h('span', { class: 'flex items-baseline gap-1.5 min-w-0' },
            h('span', { class: 'text-sm font-medium truncate', text: it.label }),
            h('span', { class: 'text-xs font-mono text-base-content/60 truncate', text: '@' + it.token })),
          hint.length && h('span', { class: 'block text-xs text-base-content/60 truncate', text: hint.join(' · ') })),
        h('span', { class: 'badge badge-ghost badge-xs shrink-0', text: core.KIND_LABEL[it.kind] || it.kind })
      );
    }

    function paint() {
      pop.textContent = '';
      if (!st.list.length) {
        pop.appendChild(h('div', {
          class: 'px-3 py-2 text-xs text-base-content/60',
          text: st.loading ? 'Loading…' : st.error ? st.error : 'Nothing matches @' + st.q.query + '.',
        }));
      }
      st.list.forEach(function (it, i) {
        pop.appendChild(row(it, i));
      });
      pop.appendChild(h('div', { class: 'neo-agent-ref-foot', text: '↑↓ choose · Enter insert · Esc close' }));
      pop.classList.remove('hidden');
      input.setAttribute('aria-expanded', 'true');
      if (st.list.length) input.setAttribute('aria-activedescendant', 'neo-agent-ref-' + st.sel);
      var act = pop.querySelector('.is-active');
      if (act && act.scrollIntoView) act.scrollIntoView({ block: 'nearest' });
    }

    function update() {
      var q = core.refQuery(input.value, input.selectionStart || 0);
      if (!q || input.selectionStart !== input.selectionEnd) return close();
      var changed = !st.open || !st.q || st.q.query !== q.query || st.q.start !== q.start;
      st.q = q;
      st.open = true;
      if (!changed) return;
      st.sel = 0;
      var seq = ++st.seq;
      st.loading = true;
      st.error = null;
      if (!st.list.length) paint();
      loadRefs().then(
        function (items) {
          if (seq !== st.seq || !st.open) return;
          st.loading = false;
          st.list = core.rankRefs(items, q.query, 8);
          paint();
        },
        function () {
          if (seq !== st.seq || !st.open) return;
          st.loading = false;
          st.list = [];
          st.error = 'References are not available right now.';
          paint();
        }
      );
    }

    function pick(i) {
      var it = st.list[i];
      if (!it || !st.q) return close();
      var caret = input.selectionStart || 0;
      var out = core.insertRef(input.value, st.q.start, caret, it.token);
      input.value = out.text;
      input.setSelectionRange(out.caret, out.caret);
      close();
      input.focus();
    }

    function key(e) {
      if (!st.open || e.isComposing) return false;
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        close();
        return true;
      }
      if (!st.list.length) return false;
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        var n = st.list.length;
        st.sel = (st.sel + (e.key === 'ArrowDown' ? 1 : n - 1)) % n;
        paint();
        return true;
      }
      if ((e.key === 'Enter' && !e.shiftKey) || e.key === 'Tab') {
        e.preventDefault();
        pick(st.sel);
        return true;
      }
      return false;
    }

    input.setAttribute('aria-autocomplete', 'list');
    input.setAttribute('aria-controls', pop.id);
    input.setAttribute('aria-expanded', 'false');
    input.addEventListener('input', update);
    input.addEventListener('click', update);
    input.addEventListener('keyup', function (e) {
      if (/^(ArrowLeft|ArrowRight|Home|End)$/.test(e.key)) update();
    });
    input.addEventListener('blur', function () {
      setTimeout(close, 120);
    });
    return { key: key, close: close };
  }

  root.NeoAgentRefs = { picker: picker };
})(window);
