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
  # Legacy: neo-service / nixos before migration 003.
  legacyService = raw."neo-service" or raw.nixos or {};
  neoInput = neoCli.neoInput or legacyService.neoInput or "github:madebydamo/neo";
  plugins = core.plugins or legacyService.plugins or [];
in {
  flake-file.inputs =
    {
      nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
      flake-file.url = lib.mkDefault "github:vic/flake-file";
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
