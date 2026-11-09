# Plugin ids and labels from core.plugins (skipped by import-tree: the leading _).
# neo imports this file too (nix/lib/plugin.nix): a config's flake inputs are
# decided before the neo input exists, so the code has to live in the template.
# The same rules exist in cli/src/utils/plugins.rs and the plugin_list widget;
# all of them run the vectors in cli/src/utils/plugin_id_vectors.json.
{lib}: let
  # Repository name for forge refs (github:o/r[/branch]), else the last path segment.
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

  # Plugin id from a label: lowercase [a-z0-9-], no leading digit.
  sanitize = s: let
    joined = lib.concatStringsSep "-" (lib.filter (p: builtins.isString p && p != "") (builtins.split "[^a-z0-9]+" (lib.toLower s)));
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
in {
  inherit label sanitize fromList normalize;
}
