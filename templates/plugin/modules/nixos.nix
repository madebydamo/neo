{config, ...}: {
  systems = ["x86_64-linux"];
  perSystem = {pkgs, ...}: {
    formatter = pkgs.alejandra;
  };
  flake = {lib, ...}: let
    allNixosModules = lib.attrValues (config.flake.modules.nixos or {});
  in {
    nixosModules.default = {
      imports = allNixosModules;
    };
  };
}
