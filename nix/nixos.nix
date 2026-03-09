# Provides the NixOS configuration machinery:
# - configurations.nixos option for declaring NixOS configs from any top-level module
# - Auto-injects all flake.modules.nixos.* into every NixOS configuration
# - Exports nixosModules.default so other flakes can import all services
# - devices option for device-specific modules (not auto-included)
# - Exports the formatter
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

  # Collect all registered NixOS deferred modules
  allNixosModules = lib.attrValues (config.flake.modules.nixos or {});
in {
  options = {
    configurations.nixos = lib.mkOption {
      type = lib.types.lazyAttrsOf (
        lib.types.submodule {
          options.modules = lib.mkOption {
            type = lib.types.listOf lib.types.deferredModule;
            default = [];
            description = "Additional NixOS modules for this configuration";
          };
        }
      );
      default = {};
      description = "NixOS configurations to build. All flake.modules.nixos.* are auto-included.";
    };

    # Device-specific NixOS modules, not auto-included in configurations.
    devices = lib.mkOption {
      type = lib.types.attrsOf lib.types.deferredModule;
      default = {};
      description = "Device-specific NixOS modules. Reference explicitly in configurations.";
    };
  };

  config.flake = {
    formatter.${system} = pkgs.alejandra;

    # Default NixOS module that bundles all services for external use.
    # Other flakes can: imports = [ neo.nixosModules.default ];
    nixosModules.default = {
      imports =
        allNixosModules
        ++ [
          inputs.nix-openclaw.nixosModules.openclaw-gateway
          {nixpkgs.overlays = [inputs.nix-openclaw.overlays.default];}
        ];
    };

    # Build NixOS configurations, auto-injecting all registered modules.
    nixosConfigurations =
      lib.mapAttrs (
        name: {modules}:
          inputs.nixpkgs.lib.nixosSystem {
            inherit system;
            specialArgs = {
              lib = extendedLib;
            };
            modules = [config.flake.nixosModules.default] ++ modules;
          }
      )
      config.configurations.nixos;
  };
}
