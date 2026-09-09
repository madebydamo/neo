// NeoWidgets — register UI widgets that live next to their Handlebars templates.
// Browser: load this file before each widget script and option_form.js.
// Node tests: require() this file first so globalThis.NeoWidgets exists.
(function (root) {
  const NeoWidgets = {
    _reg: Object.create(null),

    register(name, def) {
      if (!name || typeof name !== 'string') {
        throw new Error('NeoWidgets.register: name is required');
      }
      if (!def || typeof def !== 'object') {
        throw new Error('NeoWidgets.register: definition is required');
      }
      this._reg[name] = def;
      return def;
    },

    get(name) {
      if (!name) return undefined;
      return this._reg[name];
    },

    names() {
      return Object.keys(this._reg);
    },

    mixins() {
      const out = {};
      this.names().forEach((name) => {
        const mix = this._reg[name].mixins;
        if (mix && typeof mix === 'object') Object.assign(out, mix);
      });
      return out;
    },

    resetForTests() {
      this._reg = Object.create(null);
    },
  };

  root.NeoWidgets = NeoWidgets;
  if (typeof module === 'object' && module.exports) {
    module.exports = NeoWidgets;
  }
})(typeof globalThis !== 'undefined' ? globalThis : this);
