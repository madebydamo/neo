# Multi-machine Nix remote builders: priority ordering by weight (speedFactor),
# disabled builder exclusion, and per-builder SSH config isolation.
{
  config,
  inputs,
  lib,
  ...
}: let
  evalWith = builders:
    (inputs.nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      specialArgs.lib = config.flake.lib;
      modules = [
        config.flake.nixosModules.default
        config.flake.nixosModules.base
        {
          boot.isContainer = true;
          fileSystems."/" = {
            device = "x";
            fsType = "ext4";
          };
          system.stateVersion = "24.11";
          neo.core.nix.builders = lib.mkForce builders;
        }
      ];
    })
    .config;

  emptyCfg = evalWith {};

  multiCfg = evalWith {
    slow = {
      host = "10.0.0.10";
      user = "homeserver";
      maxJobs = 4;
      speedFactor = 1;
      systems = ["x86_64-linux"];
    };
    fast = {
      host = "10.0.0.20";
      user = "builder";
      maxJobs = 16;
      speedFactor = 5;
      systems = ["x86_64-linux" "aarch64-linux"];
      port = 2222;
      sshOptions = {
        StrictHostKeyChecking = "accept-new";
      };
      publicHostKey = "c3NoLWVkMjU1MTkgQUFBQQ==";
    };
    paused = {
      enabled = false;
      host = "10.0.0.30";
      user = "homeserver";
      speedFactor = 99;
    };
  };

  machines = multiCfg.nix.buildMachines;
  hasTwo = builtins.length machines == 2;
  first = builtins.elemAt machines 0;
  second = builtins.elemAt machines 1;
  sshCfg = multiCfg.programs.ssh.extraConfig;

  problems =
    lib.optional emptyCfg.nix.distributedBuilds "empty builders must not enable distributedBuilds"
    ++ lib.optional (emptyCfg.nix.buildMachines != []) "empty builders must leave buildMachines empty"
    ++ lib.optional (!multiCfg.nix.distributedBuilds) "enabled builders must enable distributedBuilds"
    ++ lib.optional (!(multiCfg.nix.settings.builders-use-substitutes or false)) "enabled builders must set builders-use-substitutes"
    ++ lib.optional (!hasTwo) "expected 2 active builders (paused excluded), got ${toString (builtins.length machines)}"
    ++ lib.optionals hasTwo (
      lib.optional (first.speedFactor != 5) "highest weight builder must come first (got speedFactor ${toString first.speedFactor})"
      ++ lib.optional (first.hostName != "neo-builder-fast") "builder with extraOptions must use ssh alias, got ${first.hostName}"
      ++ lib.optional (first.systems != ["x86_64-linux" "aarch64-linux"]) "first builder systems mismatch"
      ++ lib.optional (second.speedFactor != 1) "second builder speedFactor must be 1"
      ++ lib.optional (second.hostName != "10.0.0.10") "builder without extraOptions must use host directly, got ${second.hostName}"
    )
    ++ lib.optional (!(lib.hasInfix "Host neo-builder-fast" sshCfg)) "ssh config missing Host neo-builder-fast"
    ++ lib.optional (!(lib.hasInfix "HostName 10.0.0.20" sshCfg)) "ssh config missing HostName 10.0.0.20"
    ++ lib.optional (!(lib.hasInfix "HostKeyAlias neo-builder-fast" sshCfg)) "ssh config missing HostKeyAlias for publicHostKey"
    ++ lib.optional (!(lib.hasInfix "Port 2222" sshCfg)) "ssh config missing Port 2222"
    ++ lib.optional (!(lib.hasInfix "StrictHostKeyChecking accept-new" sshCfg)) "ssh config missing StrictHostKeyChecking accept-new"
    ++ lib.optional (lib.hasInfix "neo-builder-slow" sshCfg) "builder without port/sshOptions must not get ssh_config block"
    ++ lib.optional (lib.hasInfix "10.0.0.30" sshCfg) "paused builder must not appear in ssh_config";
in {
  perSystem = {pkgs, ...}: {
    checks.nix-builders = pkgs.runCommand "nix-builders" {} ''
      ${lib.concatMapStrings (p: "echo ${lib.escapeShellArg "FAIL ${p}"} >&2\n") problems}
      ${lib.optionalString (problems != []) "exit 1"}
      echo "nix-builders ok" > $out
    '';
  };
}
