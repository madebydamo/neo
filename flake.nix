{
  description = "Homeserver Docker flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    nixos-generators = {
      url = "github:nix-community/nixos-generators";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      nixos-generators,
    }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      extendedLib = nixpkgs.lib.extend (
        self: super: {
          neo = import ./lib.nix { lib = self; };
        }
      );
      homeserver = nixos-generators.nixosGenerate {
        inherit pkgs system;
        format = "docker";
        specialArgs = {
          lib = extendedLib;
        };
        modules = [
          ./core.nix
          ./device/docker-configuration.nix
          ./settings.nix
        ];
      };
    in
    {
      packages.${system} = {
        inherit homeserver;
        default = homeserver;
      };

      nixosModules = {
        default = ./core.nix;
        docker = ./docker-configuration.nix;
      };

      nixosConfigurations.homeserver = nixpkgs.lib.nixosSystem {
        inherit system;
        specialArgs = {
          lib = extendedLib;
        };
        modules = [
          ./core.nix
          ./device/vm-configuration.nix
          ./settings.nix
        ];
      };
    };
}
