# Plugin instances (docs/design/plugin-instances.md).
#
# `core.plugins` is a table of instances keyed by id. Each instance imports one
# plugin flake; a plugin that exports `neoPlugin` (flakeModules.plugin) gets a
# `plugin` module argument so it can be imported again under a prefix.
#
# label/sanitize/fromList/normalize live in the deploy template, whose flake
# inputs are decided before the neo input exists.
{lib, ...}: let
  inherit (import ../../templates/homeserver/modules/_plugin-ids.nix {inherit lib;}) label sanitize fromList normalize;

  # The `plugin` module argument of one instance.
  mkInstance = {
    id,
    url ? "",
    prefix ? "",
    ...
  }: {
    inherit id url prefix;
    # Service (and container, unit, appdata, subdomain) name of this instance.
    name = s:
      if prefix == ""
      then s
      else "${prefix}-${s}";
    # Suffix for human titles: "" or " (dev)".
    label =
      if prefix == ""
      then ""
      else " (${prefix})";
  };

  # Pass `plugin` to a module (and the modules it imports) that asks for it.
  # Module args are global, so `_module.args` cannot carry a per-instance value.
  withPluginArg = plugin: m: let
    wrapResult = r:
      if builtins.isAttrs r && r ? imports
      then r // {imports = map (withPluginArg plugin) r.imports;}
      else r;
  in
    if lib.isFunction m
    then let
      fargs = lib.functionArgs m;
    in
      lib.setFunctionArgs (args:
        wrapResult (m (
          if fargs ? plugin
          then args // {inherit plugin;}
          else args
        ))) (removeAttrs fargs ["plugin"])
    else if builtins.isPath m || (builtins.isString m && lib.hasPrefix "/" m)
    then withPluginArg plugin (import m)
    else wrapResult m;

  # One NixOS module holding every `neoPlugin.modules.<name>` of an instance.
  # The per-instance key keeps two instances from being deduplicated; `_file`
  # tags option declarations with the instance id (plugin inventory).
  instantiate = instance: modules: let
    plugin = mkInstance instance;
  in {
    imports =
      lib.mapAttrsToList (n: m: {
        _file = "neo-plugin:${plugin.id}#${n}";
        key = "neo-plugin:${plugin.id}#${n}";
        imports = [(withPluginArg plugin m)];
      })
      modules;
  };
in {
  libExtensions.plugin = {
    neo.plugin = {
      inherit label sanitize normalize mkInstance withPluginArg instantiate;
      # Valid id / prefix (also checked by the web UI and neo).
      idPattern = "[a-z][a-z0-9-]*";
      prefixPattern = "([a-z][a-z0-9]*)?";
    };
  };
}
