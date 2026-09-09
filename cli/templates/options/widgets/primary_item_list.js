// primaryItemList — listOf scalars; first entry is the primary (home) item.
(function (root) {
  if (!root.NeoWidgets) {
    throw new Error('registry.js must load before primary_item_list.js');
  }

  function pilEntryLabel(optionName) {
    return this.optUi(optionName)?.entryLabel || 'Primary';
  }

  function pilEmptyHint(optionName) {
    return this.optUi(optionName)?.emptyHint
      || 'Add at least one entry. The first is the primary.';
  }

  /** Move list index to front so it becomes the primary (home) item. */
  function pilSetPrimary(optionName, idx) {
    const list = this.ensureList(optionName);
    if (idx <= 0 || idx >= list.length) return;
    const [item] = list.splice(idx, 1);
    list.unshift(item);
    this.values[optionName] = [...list];
    this.notifyKeysFromSource(optionName);
  }

  const widget = {
    name: 'primaryItemList',
    mixins: { pilEntryLabel, pilEmptyHint, pilSetPrimary },
  };
  root.NeoWidgets.register(widget.name, widget);
  if (typeof module === 'object' && module.exports) module.exports = widget;
})(typeof globalThis !== 'undefined' ? globalThis : this);
