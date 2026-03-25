{
  inputs,
  config,
  ...
}: {
  config.flake = {...}: let
    system = "x86_64-linux";
    pkgs = inputs.nixpkgs.legacyPackages.${system};
  in {
    formatter.${system} = pkgs.alejandra;

    nixosConfigurations.neo = inputs.nixpkgs.lib.nixosSystem {
      inherit system;
      specialArgs = {
        lib = inputs.neo.lib;
      };
      modules = [
        inputs.neo.nixosModules.default
        inputs.neo.nixosModules.base
        inputs.disko.nixosModules.disko
        config.flake.modules.nixos.settings
      ];
    };
  };
}
