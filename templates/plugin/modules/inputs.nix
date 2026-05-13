{lib, ...}: let
in {
  flake-file.inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-file.url = lib.mkDefault "github:vic/flake-file";
    neo.url = "github:madebydamo/neo";
    neo.inputs.nixpkgs.follows = "nixpkgs";
  };
}
