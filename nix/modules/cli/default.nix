# Neo CLI package (crane) and systemPackages for the homeserver.
#
# `self` is one store path for the whole flake. Interpolating it, or a
# subdirectory of it, makes a derivation depend on every tracked file.
# Each input below is a `cleanSourceWith` copy hashed only from the files
# its filter keeps. Templates, static files, and CSS are runtime wrapper
# inputs, so they do not rebuild the crate.
{
  lib,
  self,
  inputs,
  ...
}: let
  # `keepFile rel base` decides files. `keepDir rel` decides child directories
  # (the root itself is always kept). Rejecting a directory skips everything under it.
  slice = name: root: keepDir: keepFile:
    lib.cleanSourceWith {
      inherit name;
      src = root;
      filter = path: type: let
        rootStr = toString root;
        rel = lib.removePrefix "${rootStr}/" (toString path);
        base = baseNameOf path;
      in
        if toString path == rootStr
        then true
        else if type == "directory"
        then keepDir rel
        else keepFile rel base;
    };

  cliRoot = self + "/cli";

  # What rustc compiles: manifest, lockfile, Rust sources, and the Nix
  # extractors pulled in with include_str!.
  cargoSrc =
    slice "neo-cli-cargo" cliRoot (
      rel: rel == "src" || lib.hasPrefix "src/" rel
    ) (
      rel: base:
        rel
        == "Cargo.toml"
        || rel == "Cargo.lock"
        || (
          lib.hasPrefix "src/" rel
          && (lib.hasSuffix ".rs" base || lib.hasSuffix ".nix" base)
        )
    );

  templateDir =
    slice "neo-cli-templates" (cliRoot + "/templates") (
      rel: baseNameOf rel != "test"
    ) (
      _rel: base:
        lib.hasSuffix ".hbs" base
        || (lib.hasSuffix ".js" base && !lib.hasSuffix ".test.js" base)
    );

  staticSrc = slice "neo-cli-static" (cliRoot + "/static") (_rel: true) (
    _rel: base: base != "neo-ui.css" && !lib.hasSuffix ".test.js" base
  );

  neoUnwrapped = pkgs: let
    craneLib = inputs.crane.mkLib pkgs;
    common = {
      pname = "neo-unwrapped";
      version = "0.1.0";
      src = cargoSrc;
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
        meta = {
          description = "Neo CLI - Rust implementation for homeserver bootstrap";
          mainProgram = "neo";
        };
      }
    );

  packageWrapper = pkgs: cfg: neoWebCss: let
    neoCli = cfg."neo-cli" or {};
    localCfg = neoCli.local or {};
    serverCfg = neoCli.server or {};
    defaultServerPath = "${cfg.core.volumes.appdata or "/var/neo/DATA/AppData"}/configuration";
    defaults = pkgs.writeText "default-settings.toml" ''
      [neo-cli]
      neoInput = "${neoCli.neoInput or "github:madebydamo/neo"}"
      template = "${neoCli.template or "github:madebydamo/neo#homeserver"}"
      repoUrl = "${neoCli.repoUrl or ""}"
      defaultBranch = "${neoCli.defaultBranch or "master"}"
      rebuildBranchFormat = "${neoCli.rebuildBranchFormat or "%Y%m%d-%H%M%S"}"
      [neo-cli.local]
      configPath = "${localCfg.configPath or "./build"}"
      neoInput = "${localCfg.neoInput or "github:madebydamo/neo"}"
      template = "${localCfg.template or ""}"
      [neo-cli.server]
      configPath = "${serverCfg.configPath or defaultServerPath}"
      neoInput = "${serverCfg.neoInput or "github:madebydamo/neo"}"
      template = "${serverCfg.template or ""}"
      [core.git]
      userName = ${builtins.toJSON (cfg.core.git.userName or "Neo")}
      userEmail = ${builtins.toJSON (cfg.core.git.userEmail or "neo@local")}
      [disko]
      enabled = ${
        if cfg.disko.enabled or false
        then "true"
        else "false"
      }
    '';
    staticDir = pkgs.runCommand "neo-static" {} ''
      mkdir -p $out
      cp -a ${staticSrc}/. $out/
      chmod -R u+w $out
      rm -f $out/neo-ui.css
      cp ${neoWebCss}/neo-ui.css $out/neo-ui.css
    '';
    unwrapped = neoUnwrapped pkgs;
  in
    pkgs.runCommand "neo-0.1.0" {
      nativeBuildInputs = [pkgs.makeWrapper];
      passthru = {inherit unwrapped cargoSrc;};
      meta = {
        description = "Neo CLI - Rust implementation for homeserver bootstrap";
        mainProgram = "neo";
      };
    } ''
      mkdir -p $out/bin
      makeWrapper ${unwrapped}/bin/neo $out/bin/neo \
        --inherit-argv0 \
        --set TEMPLATE_DIR ${templateDir} \
        --set STATIC_DIR ${staticDir} \
        --set DEFAULT_SETTINGS_PATH ${defaults} \
        --set-default SOPS_BINARY_PATH ${pkgs.sops}/bin/sops
    '';
  cfgPackage = self.nixosConfigurations.homeserver.config.neo;
in {
  perSystem = {
    pkgs,
    config,
    ...
  }: {
    packages.neo = packageWrapper pkgs cfgPackage config.packages.neo-web-css;
  };
  flake.modules.nixos.cli = {
    lib,
    pkgs,
    config,
    ...
  }: {
    config = let
      cfg = config.neo;
      system = pkgs.stdenv.hostPlatform.system;
      neoWebCss = self.packages.${system}.neo-web-css;
      neoCli = packageWrapper pkgs cfg neoWebCss;
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
