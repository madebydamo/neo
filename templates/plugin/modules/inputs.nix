{...}: {
  flake-file.inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    neo.url = "github:madebydamo/neo";
    neo.inputs.nixpkgs.follows = "nixpkgs";
  };
}
