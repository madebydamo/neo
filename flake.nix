{
  description = "Homeserver Nix flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nix-openclaw.url = "github:openclaw/nix-openclaw";
    nix-openclaw.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    nix-openclaw,
  }: let
    system = "x86_64-linux";
    pkgs = nixpkgs.legacyPackages.${system};
    extendedLib = nixpkgs.lib.extend (
      self: super: {
        neo = import ./lib.nix {
          lib = self;
        };
      }
    );
  in {
    formatter.${system} = pkgs.alejandra;

    nixosModules = {
      default = ./core.nix;
    };

    nixosConfigurations.homeserver = nixpkgs.lib.nixosSystem {
      inherit system;
      specialArgs = {
        lib = extendedLib;
      };
      modules = [
        nix-openclaw.nixosModules.openclaw-gateway
        {nixpkgs.overlays = [nix-openclaw.overlays.default];}
        ./core.nix
        ./device/vm-configuration.nix
        ./settings.nix
      ];
    };
  };
}
