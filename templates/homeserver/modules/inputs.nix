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
  neoService = raw."neo-service" or raw.nixos or {};
  neoInput = neoService.neoInput or "github:madebydamo/neo";
  plugins = neoService.plugins or [];
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
