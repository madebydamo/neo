{
  lib,
  self,
  inputs,
  ...
}: let
  packageWrapper = pkgs: cfg: let
    craneLib = inputs.crane.mkLib pkgs;
    src = lib.cleanSourceWith {
      src = self + "/cli";
      filter = path: type:
        (craneLib.filterCargoSources path type)
        || (lib.hasSuffix ".nix" path);
    };
    defaults = pkgs.writeText "default-settings.toml" ''
      [neo-service]
      enabled = ${
        if (cfg."neo-service" or {}).enabled or false
        then "true"
        else "false"
      }
      configPath = "${(cfg."neo-service" or {}).configPath or "/var/neo/DATA/AppData/configuration"}"
      neoInput = "${(cfg."neo-service" or {}).neoInput or "github:madebydamo/neo"}"
      template = "${(cfg."neo-service" or {}).template or "github:madebydamo/neo#homeserver"}"
      bootstrapEnabled = ${
        if (cfg."neo-service" or {}).bootstrapEnabled or false
        then "true"
        else "false"
      }
      autoUpdateEnabled = ${
        if (cfg."neo-service" or {}).autoUpdateEnabled or false
        then "true"
        else "false"
      }
      bootstrapMethod = "${(cfg."neo-service" or {}).bootstrapMethod or "template"}"
      repoUrl = "${(cfg."neo-service" or {}).repoUrl or ""}"
      gitUserName = "${(cfg."neo-service" or {}).gitUserName or "Neo Bootstrap"}"
      gitUserEmail = "${(cfg."neo-service" or {}).gitUserEmail or "neo@local"}"
      defaultBranch = "${(cfg."neo-service" or {}).defaultBranch or "master"}"
      [neo-cli]
      configPath = "${(cfg."neo-cli" or {}).configPath or "./build"}"
      template = "${(cfg."neo-cli" or {}).template or "..#homeserver"}"
      bootstrapMethod = "${(cfg."neo-cli" or {}).bootstrapMethod or "template"}"
      repoUrl = "${(cfg."neo-cli" or {}).repoUrl or ""}"
      [disko]
      enabled = ${
        if cfg.disko.enabled or false
        then "true"
        else "false"
      }
    '';
    template_dir = "${self}/cli/templates";
    static_dir = "${self}/cli/static";
    common = {
      pname = "neo";
      version = "0.1.0";
      inherit src;
      nativeBuildInputs = [
        pkgs.pkg-config
        pkgs.git
      ];
      buildInputs = [pkgs.openssl];
      doCheck = false;
    };
    deps = craneLib.buildDepsOnly common;
  in
    craneLib.buildPackage (
      common
      // {
        cargoArtifacts = deps;
        env.DEFAULT_SETTINGS_TOML = builtins.readFile defaults;
        env.NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
        env.SUDO_BINARY_PATH = "${pkgs.sudo}/bin/sudo";
        env.TEMPLATE_DIR = template_dir;
        env.STATIC_DIR = static_dir;
        meta = {
          description = "Neo CLI - Rust implementation for homeserver bootstrap";
          mainProgram = "neo";
        };
      }
    );
  cfgPackage = self.nixosConfigurations.homeserver.config.neo;
in {
  perSystem = {pkgs, ...}: {
    packages.neo = packageWrapper pkgs cfgPackage;
  };
  flake.modules.nixos.cli = {
    lib,
    pkgs,
    config,
    ...
  }: {
    config = let
      cfg = config.neo;
      neoCli = packageWrapper pkgs cfg;
    in {
      environment.systemPackages = [
        neoCli
        pkgs.git
        pkgs.nix
        pkgs.nixos-install-tools
        pkgs.coreutils
        pkgs.lazygit
        pkgs.vim
      ];
    };
  };
}
