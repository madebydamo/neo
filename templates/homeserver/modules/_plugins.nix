# core.plugins from settings.toml as { <id> = { url; prefix; }; } (skipped by
# import-tree: the leading _). inputs.nix and nixos.nix both read it.
{lib}: let
  inherit (import ./_plugin-ids.nix {inherit lib;}) normalize;
  settingsPath = ../settings.toml;
  raw =
    if builtins.pathExists settingsPath
    then builtins.fromTOML (builtins.readFile settingsPath)
    else {};
in
  normalize ((raw.core or {}).plugins or {})
