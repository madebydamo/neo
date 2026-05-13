{
  config,
  lib,
  ...
}: let
  settingsPath = ../settings.toml;
  neo =
    if builtins.pathExists settingsPath
    then builtins.fromTOML (builtins.readFile settingsPath)
    else {};
  neoInput = neo.nixos.neoInput or "github:madebydamo/neo";
  plugins = neo.nixos.plugins or [];
in {
  flake-file.inputs =
    {
      nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
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
