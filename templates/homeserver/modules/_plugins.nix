# core.plugins from settings.toml as { <id> = { url; prefix; }; } (skipped by
# import-tree: the leading _). inputs.nix and nixos.nix both read it.
# Same rules as lib.neo.plugin.normalize in neo (nix/lib/plugin.nix): inputs are
# decided before the neo input exists, so the template keeps its own copy.
{lib}: let
  label = url: let
    trimmed = lib.trim url;
    path = lib.removeSuffix "/" (lib.head (builtins.split "[?#]" trimmed));
    prefixes = ["git+file:" "git+https://" "git+http://" "https://" "http://" "path:" "github:" "gitlab:" "sourcehut:"];
    rest = lib.removePrefix (lib.findFirst (p: lib.hasPrefix p path) "" prefixes) path;
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
  sanitize = s: let
    joined = lib.concatStringsSep "-" (lib.filter (p: builtins.isString p && p != "") (builtins.split "[^a-z0-9]+" (lib.toLower s)));
  in
    if joined == ""
    then "plugin"
    else if builtins.match "[0-9].*" joined != null
    then "plugin-${joined}"
    else joined;
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
  normalize = v:
    if builtins.isList v
    then fromList v
    else
      lib.mapAttrs (_: p: {
        url = p.url or "";
        prefix = p.prefix or "";
      })
      v;
  settingsPath = ../settings.toml;
  raw =
    if builtins.pathExists settingsPath
    then builtins.fromTOML (builtins.readFile settingsPath)
    else {};
in
  normalize ((raw.core or {}).plugins or {})
