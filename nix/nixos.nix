# Provides an option for declaring NixOS configurations and a formatter.
# NixOS configurations end up as flake outputs under `#nixosConfigurations."<name>"`.
{
  lib,
  config,
  inputs,
  ...
}: let
  system = "x86_64-linux";
  pkgs = inputs.nixpkgs.legacyPackages.${system};
  extendedLib = inputs.nixpkgs.lib.extend (
    self: super: {
      neo = import ./_lib.nix {
        lib = self;
      };
    }
  );
in {
  options.configurations.nixos = lib.mkOption {
    type = lib.types.lazyAttrsOf (
      lib.types.submodule {
        options.modules = lib.mkOption {
          type = lib.types.listOf lib.types.deferredModule;
          default = [];
          description = "List of NixOS modules for this configuration";
        };
      }
    );
    default = {};
    description = "NixOS configurations to build";
  };

  config.flake = {
    formatter.${system} = pkgs.alejandra;

    nixosConfigurations =
      lib.mapAttrs (
        name: {modules}:
          inputs.nixpkgs.lib.nixosSystem {
            inherit system;
            specialArgs = {
              lib = extendedLib;
            };
            inherit modules;
          }
      )
      config.configurations.nixos;
  };
}
