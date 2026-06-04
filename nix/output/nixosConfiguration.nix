# Provides the NixOS configuration machinery:
# - configurations.nixos option for declaring NixOS configs from any top-level module
# - Auto-injects all flake.modules.nixos.* into every NixOS configuration
# - Exports nixosModules.default so other flakes can import all services
# - devices option for device-specific modules (not auto-included)
{
  lib,
  config,
  inputs,
  ...
}: let
  allNixosModules = lib.attrValues (config.flake.modules.nixos or {});
  nixosModulesDefault = {
    imports = allNixosModules;
  };
  system = "x86_64-linux";
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
    nixosModules.default = nixosModulesDefault;

    nixosConfigurations =
      lib.mapAttrs (
        name: {modules}:
          inputs.nixpkgs.lib.nixosSystem {
            inherit system;
            specialArgs = {
              lib = config.flake.lib;
            };
            modules = [nixosModulesDefault] ++ modules;
          }
      )
      config.configurations.nixos;
  };
}
