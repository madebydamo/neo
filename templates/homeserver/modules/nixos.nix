{
  inputs,
  config,
  ...
}: {
  systems = ["x86_64-linux"];
  perSystem = {
    pkgs,
    inputs',
    ...
  }: {
    formatter = pkgs.alejandra;
    packages.neo = inputs'.neo.packages.neo;
  };
  flake = {lib, ...}: let
    system = "x86_64-linux";
    hardwareConfig =
      if builtins.pathExists ../hardware-configuration.nix
      then [../hardware-configuration.nix]
      else [];
    pluginModules = map (n: inputs.${n}.nixosModules.default) (builtins.filter (n: lib.hasPrefix "plugin" n) (builtins.attrNames inputs));
  in {
    nixosConfigurations.neo = inputs.nixpkgs.lib.nixosSystem {
      inherit system;
      specialArgs = {
        lib = inputs.neo.lib;
      };
      modules =
        [
          inputs.neo.nixosModules.default
          inputs.neo.nixosModules.base
          config.flake.modules.nixos.settings
        ]
        ++ hardwareConfig ++ pluginModules;
    };
    nixosConfigurations.vm = inputs.nixpkgs.lib.nixosSystem {
      inherit system;
      specialArgs = {
        lib = inputs.neo.lib;
      };
      modules =
        [
          inputs.neo.nixosModules.default
          inputs.neo.nixosModules.base
          inputs.neo.nixosModules.vm
          config.flake.modules.nixos.settings
        ]
        ++ pluginModules;
    };
  };
}
