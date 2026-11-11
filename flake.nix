{
  description = "Homeserver Nix flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    disko = {
      url = "github:nix-community/disko";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    import-tree.url = "github:denful/import-tree";
    # Pinned to the last revision before the switch to pnpm (2026-11-06):
    # later main does not evaluate on the nixpkgs release neo follows
    # (pnpm.override missing). Unpin once it does and
    # NousResearch/hermes-agent#152542 (patchelf patch vs. patchelf 0.15) is
    # merged; until then see the patchelf swap in nix/services/hermes/default.nix.
    hermes-agent = {
      url = "github:NousResearch/hermes-agent/44ec783afd1193afb7576d9c01d7fe9d18c87339";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.pyproject-nix.follows = "pyproject-nix";
    };
    # Owned here so hermes picks up build-hook fixes before its own lock does
    # (e.g. UV_SYSTEM_CERTS=0 for the sandbox after nixpkgs wrapped uv).
    pyproject-nix = {
      url = "github:pyproject-nix/pyproject.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    crane.url = "github:ipetkov/crane";
  };

  outputs = inputs: inputs.flake-parts.lib.mkFlake {inherit inputs;} (inputs.import-tree ./nix);
}
