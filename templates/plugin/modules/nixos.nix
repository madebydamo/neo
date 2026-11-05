# Every neoPlugin.modules.* under modules/ is a NixOS module with an extra
# `plugin` argument. inputs.neo.flakeModules.plugin (imports.nix) exports them as
# neoPlugin (for Neo) and nixosModules.default (for older Neo hosts).
# A homeserver can add this plugin twice under different prefixes, so every name
# a module puts into the system goes through `plugin.name "<service>"`.
{...}: {
  systems = ["x86_64-linux"];
  perSystem = {pkgs, ...}: {
    formatter = pkgs.alejandra;
  };
}
