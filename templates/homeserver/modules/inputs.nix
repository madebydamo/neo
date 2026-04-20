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
in {
  flake-file.inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-file.url = lib.mkDefault "github:vic/flake-file";
    neo.url = neoInput;
    neo.inputs.nixpkgs.follows = "nixpkgs";
  };
}
