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
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-file.url = lib.mkDefault "github:vic/flake-file";
    neo.url = neoInput;
    disko.url = "github:nix-community/disko";
    disko.inputs.nixpkgs.follows = "nixpkgs";
  };
}
