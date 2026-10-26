{
  config,
  lib,
  ...
}: let
  settingsPath = ../settings.toml;
  raw =
    if builtins.pathExists settingsPath
    then builtins.fromTOML (builtins.readFile settingsPath)
    else {};
  core = raw.core or {};
  neoCli = raw."neo-cli" or {};
  localCli = neoCli.local or {};
  serverCli = neoCli.server or {};
  # Directory of a local flake ref (git+file:, path:, file:, or an absolute path).
  # The part after # is a flake output name, not part of the directory.
  # Mirrors local_flake_dir / is_local_flake_ref in the neo CLI (cli/src/utils/profile.rs).
  localBase = url: let
    ref = lib.head (lib.splitString "#" url);
    prefix = lib.findFirst (p: lib.hasPrefix p ref) "" ["git+file://" "git+file:" "path:" "file://" "file:"];
  in
    lib.removePrefix prefix ref;
  localDir = url: let
    dir = localBase url;
  in
    if lib.hasPrefix "/" dir
    then dir
    else null;
  # Relative refs (./, ../) only exist next to a checkout, never on the server.
  isLocalRef = url: let
    dir = localBase url;
  in
    lib.hasPrefix "/" dir || lib.hasPrefix "./" dir || lib.hasPrefix "../" dir;
  localUrl = localCli.neoInput or "";
  localPath = localDir localUrl;
  # Laptop builds follow the checkout when that directory exists. On the
  # homeserver the path is absent, so the server input (GitHub by default) wins.
  # Pure eval hides paths outside this repo (pathExists is false), so neo runs
  # `nix run --impure .#write-flake` for this check. `--impure` follows `run`.
  useLocal = localPath != null && builtins.pathExists localPath;
  sharedUrl = neoCli.neoInput or "";
  sharedIsLocal = isLocalRef sharedUrl;
  serverUrl =
    if (serverCli.neoInput or "") != ""
    then serverCli.neoInput
    else if sharedUrl != "" && !sharedIsLocal
    then sharedUrl
    else "github:madebydamo/neo";
  neoInput =
    if useLocal
    then localUrl
    else serverUrl;
  plugins = core.plugins or [];
in {
  flake-file.inputs =
    {
      nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
      neo.url = neoInput;
      neo.inputs.nixpkgs.follows = "nixpkgs";
    }
    // lib.listToAttrs (lib.imap0 (i: p: {
        name = "plugin${toString i}";
        value = {
          url = p;
          inputs.neo.follows = "neo";
          inputs.nixpkgs.follows = "nixpkgs";
          inputs.flake-file.follows = "flake-file";
        };
      })
      plugins);
}
