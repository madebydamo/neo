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
    system = lib.head (config.systems or ["x86_64-linux"]);
    hardwareConfig =
      if builtins.pathExists ../hardware-configuration.nix
      then [../hardware-configuration.nix]
      else [];
    # One module per core.plugins instance (docs/design/plugin-instances.md).
    # Plugins with neoPlugin get their prefix; older ones are imported as they are.
    # A plugin saved in settings.toml has no input until the next write-flake, which
    # every activate and build runs first. Until then it is simply not loaded, so the
    # web UI, which evaluates this flake right after a save, keeps working.
    pluginModules = lib.mapAttrsToList (id: p: let
      flake = inputs."plugin-${id}";
    in
      if flake ? neoPlugin
      then
        if p.prefix != "" && !(flake.neoPlugin.prefixable or true)
        then throw "Plugin '${id}' (${p.url}) still has modules that cannot take a prefix. Remove the prefix."
        else
          flake.neoPlugin.module {
            inherit id;
            inherit (p) url prefix;
          }
      else if p.prefix == ""
      then flake.nixosModules.default
      else throw "Plugin '${id}' (${p.url}) does not support a prefix. Remove the prefix or update the plugin.")
    (lib.filterAttrs (id: _: inputs ? "plugin-${id}") (import ./_plugins.nix {inherit lib;}));
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
