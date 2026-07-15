# Neo CLI package (crane) and systemPackages for the homeserver.
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
    neoCli = cfg."neo-cli" or {};
    localCfg = neoCli.local or {};
    serverCfg = neoCli.server or {};
    defaultServerPath = "${cfg.core.volumes.appdata or "/var/neo/DATA/AppData"}/configuration";
    defaults = pkgs.writeText "default-settings.toml" ''
      [neo-cli]
      neoInput = "${neoCli.neoInput or "github:madebydamo/neo"}"
      template = "${neoCli.template or "github:madebydamo/neo#homeserver"}"
      bootstrapMethod = "${neoCli.bootstrapMethod or "template"}"
      repoUrl = "${neoCli.repoUrl or ""}"
      gitUserName = "${neoCli.gitUserName or "Neo Bootstrap"}"
      gitUserEmail = "${neoCli.gitUserEmail or "neo@local"}"
      defaultBranch = "${neoCli.defaultBranch or "master"}"
      rebuildBranchFormat = "${neoCli.rebuildBranchFormat or "%Y%m%d-%H%M%S"}"
      [neo-cli.local]
      configPath = "${localCfg.configPath or "./build"}"
      [neo-cli.server]
      configPath = "${serverCfg.configPath or defaultServerPath}"
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
