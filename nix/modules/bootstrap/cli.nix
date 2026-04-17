{
  lib,
  self,
  ...
}: let
  packageWrapper = pkgs: cfg: let
    src = self + "/cli";
    defaults = pkgs.writeText "default-settings.toml" ''
      [nixos]
      enabled = ${
        if cfg.nixos.enabled or false
        then "true"
        else "false"
      }
      configPath = "${cfg.nixos.configPath or "/var/neo/DATA/AppData/configuration"}"
      neoInput = "${cfg.nixos.neoInput or "github:madebydamo/neo"}"
      template = "${cfg.nixos.template or "github:madebydamo/neo#homeserver"}"
      bootstrapEnabled = ${
        if cfg.nixos.bootstrapEnabled or false
        then "true"
        else "false"
      }
      autoUpdateEnabled = ${
        if cfg.nixos.autoUpdateEnabled or false
        then "true"
        else "false"
      }
      bootstrapMethod = "${cfg.nixos.bootstrapMethod or "template"}"
      repoUrl = "${cfg.nixos.repoUrl or ""}"
      gitUserName = "${cfg.nixos.gitUserName or "Neo Bootstrap"}"
      gitUserEmail = "${cfg.nixos.gitUserEmail or "neo@local"}"
      defaultBranch = "${cfg.nixos.defaultBranch or "master"}"
      [cli]
      configPath = "${cfg.cli.configPath or "./build"}"
      template = "${cfg.cli.template or "..#homeserver"}"
      bootstrapMethod = "${cfg.cli.bootstrapMethod or "template"}"
      repoUrl = "${cfg.cli.repoUrl or ""}"
      [disko]
      enabled = ${
        if cfg.disko.enabled or false
        then "true"
        else "false"
      }
    '';
    # userOverrides = ../../../settings.toml;
    userOverrides =
      if builtins.pathExists ../../../settings.toml
      then builtins.readFile ../../../settings.toml
      else "";
  in
    pkgs.rustPlatform.buildRustPackage {
      pname = "neo";
      version = "0.1.0";
      inherit src;
      cargoLock = {
        lockFile = src + "/Cargo.lock";
      };
      env.DEFAULT_SETTINGS_TOML = builtins.readFile defaults;
      env.USER_OVERRIDE_SETTINGS_TOML = userOverrides;
      env.NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
      env.SUDO_BINARY_PATH = "${pkgs.sudo}/bin/sudo";
      meta = {
        description = "Neo CLI - Rust implementation for homeserver bootstrap";
        mainProgram = "neo";
      };
      doCheck = false;
      nativeBuildInputs = [
        pkgs.pkg-config
        pkgs.git
      ];
      buildInputs = [pkgs.openssl];
    };
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
      ];
      environment.variables = {
        NEO_SETTINGS = "${cfg.nixos.configPath}/settings.toml";
        NEO_SECTION = "nixos";
      };
    };
  };
}
