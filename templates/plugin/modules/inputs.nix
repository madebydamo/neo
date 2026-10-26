# Inputs for flake.nix (regenerate it with `nix run .#write-flake`).
# A homeserver adds this plugin with neo, nixpkgs and flake-file following its own
# inputs, so these URLs only matter when the plugin is evaluated on its own.
{...}: {
  flake-file.inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    neo.url = "github:madebydamo/neo";
    neo.inputs.nixpkgs.follows = "nixpkgs";
  };
}
