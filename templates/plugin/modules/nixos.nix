# Every flake.modules.nixos.* under modules/ is exported as nixosModules.default,
# which the homeserver imports into its NixOS configuration (with lib.neo available).
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
