# flakeModules.plugin: what a plugin flake imports to be loaded by Neo.
#
# Modules under `neoPlugin.modules.<name>` receive a `plugin` argument
# (lib.neo.plugin.mkInstance) and can be imported several times under different
# prefixes. Modules under `flake.modules.nixos.*` are still exported as before;
# a plugin that has any of those can only be imported once, without a prefix.
#
# checks.<system>.neo-plugin-instances evaluates Neo with the plugin once and
# twice (prefixes neoa and neob) and fails on names that did not get the prefix.
# See docs/design/plugin-instances.md.
{
  config,
  inputs,
  ...
}: let
  neoLib = config.flake.lib;
  neoModules = [config.flake.nixosModules.default config.flake.nixosModules.base];
  neoNixpkgs = inputs.nixpkgs;
in {
  flake.flakeModules.plugin = {
    config,
    lib,
    inputs,
    ...
  }: let
    legacy = lib.attrValues (config.flake.modules.nixos or {});
    module = instance: {
      imports = [(neoLib.neo.plugin.instantiate instance config.neoPlugin.modules)] ++ legacy;
    };
    prefixable = legacy == [];

    nixpkgs = inputs.nixpkgs or neoNixpkgs;
    evalWith = instances: enable:
      (nixpkgs.lib.nixosSystem {
        system = "x86_64-linux";
        specialArgs.lib = neoLib;
        modules =
          neoModules
          ++ map module instances
          ++ [
            {
              neo.services =
                lib.genAttrs enable (_: {enabled = true;})
                // {swag.domain = "example.org";};
            }
          ];
      }).config;

    instanceCheck = pkgs: let
      base = evalWith [] [];
      names = lib.subtractLists (lib.attrNames base.neo.services) (lib.attrNames (evalWith [{id = "one";}] []).neo.services);
      single = evalWith [{id = "one";}] names;
      # Unusual prefixes, so a name that carries one can only have it from plugin.name.
      double = evalWith [
        {
          id = "a";
          prefix = "neoa";
        }
        {
          id = "b";
          prefix = "neob";
        }
      ] (map (n: "neoa-${n}") names ++ map (n: "neob-${n}") names);
      sets = {
        "neo.services" = c: c.neo.services;
        "containers" = c: c.virtualisation.oci-containers.containers;
        "systemd.services" = c: c.systemd.services;
        "systemd.timers" = c: c.systemd.timers;
        "users.users" = c: c.users.users;
        "users.groups" = c: c.users.groups;
      };
      # "docker-neoa-example" → "docker-example"; null without the prefix.
      unprefix = p: n: let
        parts = lib.splitString "${p}-" n;
      in
        if lib.length parts == 2
        then lib.concatStrings parts
        else null;
      problems = lib.concatLists (lib.mapAttrsToList (what: get: let
        baseNames = lib.attrNames (get base);
        own = lib.subtractLists baseNames (lib.attrNames (get single));
        got = lib.subtractLists baseNames (lib.attrNames (get double));
        leaked = lib.filter (n: unprefix "neoa" n == null && unprefix "neob" n == null) got;
        missing = lib.filter (n: !(lib.elem n (map (unprefix "neoa") got) && lib.elem n (map (unprefix "neob") got))) own;
      in
        lib.optional (leaked != []) "${what}: ${toString leaked} (the same in every copy)"
        ++ lib.optional (missing != []) "${what}: ${toString missing} (not created once per copy)")
      sets);
    in
      pkgs.runCommand "neo-plugin-instances" {} (
        if problems == []
        then "touch $out"
        else ''
          cat >&2 <<'EOF'
          This plugin cannot be added twice: these names do not go through plugin.name.
          ${lib.concatStringsSep "\n" problems}
          EOF
          exit 1
        ''
      );
  in {
    options.neoPlugin.modules = lib.mkOption {
      type = lib.types.lazyAttrsOf lib.types.raw;
      default = {};
      description = "NixOS modules of this plugin. Each gets a `plugin` argument; name every service, container, unit and directory with `plugin.name \"<service>\"`.";
    };
    config = {
      flake = {
        neoPlugin = {
          inherit module prefixable;
        };
        # Hosts that predate plugin instances import this.
        nixosModules.default = module {id = "default";};
      };
      perSystem = {pkgs, ...}: {
        checks = lib.optionalAttrs (prefixable && config.neoPlugin.modules != {}) {
          neo-plugin-instances = instanceCheck pkgs;
        };
      };
    };
  };
}
