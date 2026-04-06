{
  inputs,
  config,
  ...
}: {
  systems = ["x86_64-linux"];
  perSystem = {
    config,
    pkgs,
    ...
  }: {
    formatter = pkgs.alejandra;
  };
  flake = {...}: let
    system = "x86_64-linux";
    hardwareConfig =
      if builtins.pathExists ../hardware-configuration.nix
      then [../hardware-configuration.nix]
      else [];
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
          inputs.disko.nixosModules.disko
          config.flake.modules.nixos.settings
        ]
        ++ hardwareConfig;
    };
    nixosConfigurations.vm = inputs.nixpkgs.lib.nixosSystem {
      inherit system;
      specialArgs = {
        lib = inputs.neo.lib;
      };
      modules = [
        inputs.neo.nixosModules.default
        inputs.neo.nixosModules.base
        inputs.neo.nixosModules.vm
        inputs.disko.nixosModules.disko
        config.flake.modules.nixos.settings
      ];
    };
  };
}
