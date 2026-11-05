# Plugin instances (docs/design/plugin-instances.md).
#
# `core.plugins` is a table of instances keyed by id. Each instance imports one
# plugin flake; a plugin that exports `neoPlugin` (flakeModules.plugin) gets a
# `plugin` module argument so it can be imported again under a prefix.
#
# normalize/label/sanitize are mirrored in templates/homeserver/modules/_plugins.nix
# (inputs are decided before the neo input exists) and cli/src/commands/web/plugins.rs.
{lib, ...}: let
  # Repository name for forge refs (github:o/r[/branch]), else the last path segment.
  label = url: let
    trimmed = lib.trim url;
    noQuery = lib.head (builtins.split "[?#]" trimmed);
    path = lib.removeSuffix "/" noQuery;
    prefixes = ["git+file:" "git+https://" "git+http://" "https://" "http://" "path:" "github:" "gitlab:" "sourcehut:"];
    prefix = lib.findFirst (p: lib.hasPrefix p path) "" prefixes;
    rest = lib.removePrefix prefix path;
    segments = lib.filter (s: s != "") (lib.splitString "/" rest);
    forge = lib.any (p: lib.hasPrefix p trimmed) ["github:" "gitlab:" "sourcehut:"];
    name =
      if forge && lib.length segments >= 2
      then lib.elemAt segments 1
      else if segments != []
      then lib.last segments
      else trimmed;
  in
    lib.removeSuffix ".git" name;

  # Plugin id from a label: lowercase [a-z0-9-], no leading digit.
  sanitize = s: let
    parts = lib.filter (p: builtins.isString p && p != "") (builtins.split "[^a-z0-9]+" (lib.toLower s));
    joined = lib.concatStringsSep "-" parts;
  in
    if joined == ""
    then "plugin"
    else if builtins.match "[0-9].*" joined != null
    then "plugin-${joined}"
    else joined;

  # Legacy list form: ids from the repository name, `-<index>` on a clash.
  fromList = urls: let
    ids = map (u: sanitize (label u)) urls;
    clash = id: lib.length (lib.filter (x: x == id) ids) > 1;
  in
    lib.listToAttrs (lib.imap0 (i: url: let
        id = lib.elemAt ids i;
      in {
        name =
          if clash id
          then "${id}-${toString i}"
          else id;
        value = {
          inherit url;
          prefix = "";
        };
      })
      urls);

  # core.plugins as written in settings.toml (list or table) → { <id> = { url; prefix; }; }
  normalize = v:
    if builtins.isList v
    then fromList v
    else
      lib.mapAttrs (_: p: {
        url = p.url or "";
        prefix = p.prefix or "";
      })
      v;

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
