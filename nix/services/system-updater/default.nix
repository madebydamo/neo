# System auto-updater: scheduled neo update/activate.
# The config repo is bootstrapped by neo-bootstrap (neo web service).
{self, ...}: {
  flake.modules.nixos.system-updater = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.services.system-updater;
    updaterPaths = lib.neo.mkUpdaterPaths config.neo.core.volumes.appdata;
    systemHistoryDir = cfg.appdata;
    neo = self.packages.${pkgs.stdenv.hostPlatform.system}.neo;
    activationMode = cfg.activationMode;
    rebootAfterBoot = cfg.rebootAfterBoot;
    rebootWhenRequired = cfg.rebootWhenRequired;
    # Check after a restart an update needed (restart-check.sh).
    restartCheck = pkgs.writeShellScriptBin "neo-restart-check" ''
      PATH=${lib.makeBinPath [pkgs.coreutils pkgs.gawk pkgs.jq pkgs.systemd]}:$PATH
      export NEO_UPDATER_DIR=''${NEO_UPDATER_DIR:-${systemHistoryDir}}
      exec ${pkgs.bash}/bin/bash ${./restart-check.sh} "$@"
    '';
    path = [
      neo
      pkgs.git
      pkgs.nix
      pkgs.nixos-rebuild
      pkgs.nixos-install-tools
      pkgs.coreutils
      pkgs.bash
      pkgs.jq
      pkgs.systemd
    ];
    environment = {
      NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
      SUDO_BINARY_PATH = "/run/wrappers/bin/sudo";
      # Scheduled run: wait (up to 1h) for a running activation / restore / … to
      # finish instead of failing on the operation lock like an interactive call.
      NEO_LOCK_WAIT = "3600";
      NEO_ACTIVATION_MODE = activationMode;
    };
  in {
    config = lib.mkIf cfg.enabled {
      # Keep build outputs of live derivations so build-time deps (e.g. crane/cargo
      # crate builds) are not recompiled after each scheduled GC.
      nix.settings.keep-outputs = lib.mkIf (cfg.garbageCollectOlderThan != null) true;

      system.activationScripts.neo-system-updater-history = lib.neo.mkUpdaterHistoryDirs config [updaterPaths.stateDir systemHistoryDir];

      systemd.services.neo-auto-update = {
        description = "Auto update and activate nixos config with neo";
        # neo-bootstrap is part of the neo web service. Wait for it when that
        # service is on so a scheduled update does not start without a config repo.
        wants = lib.optional config.neo.services.neo.enabled "neo-bootstrap.service";
        after = lib.optional config.neo.services.neo.enabled "neo-bootstrap.service";
        inherit path;
        inherit environment;
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = false;
          User = "homeserver";
          Group = "homeserver";
        };
        stopIfChanged = false;
        restartIfChanged = false;
        script = lib.getExe (pkgs.writeShellApplication {
          name = "neo-auto-update";
          runtimeInputs = [pkgs.coreutils pkgs.jq];
          runtimeEnv = {
            NEO_HISTORY_DIR = systemHistoryDir;
            NEO_ACTIVATION_MODE = activationMode;
            NEO_REBOOT_WHEN_REQUIRED = lib.boolToString rebootWhenRequired;
            NEO_REBOOT_AFTER_BOOT = lib.boolToString rebootAfterBoot;
            NEO_GC_OLDER_THAN = toString cfg.garbageCollectOlderThan;
            NEO_CLI = "${neo}/bin/neo";
            NEO_RESTART_CHECK = "${restartCheck}/bin/neo-restart-check";
          };
          text = builtins.readFile ./auto-update.sh;
        });
      };

      # A few minutes after every boot: when the last update needed this
      # restart, record whether the server came back on the new system
      # (restart_check in the run's history; Hermes reads it).
      systemd.services.neo-auto-update-verify = {
        description = "Check the server after a restart an update needed";
        restartIfChanged = false;
        serviceConfig = {
          Type = "oneshot";
          User = "homeserver";
          Group = "homeserver";
          TimeoutStartSec = "20min";
          ExecStart = "${restartCheck}/bin/neo-restart-check verify";
        };
      };

      systemd.timers.neo-auto-update-verify = {
        wantedBy = ["timers.target"];
        timerConfig = {
          OnBootSec = "2min";
          AccuracySec = "30s";
          Unit = "neo-auto-update-verify.service";
        };
      };

      environment.systemPackages = [restartCheck];

      systemd.timers.neo-auto-update = {
        wantedBy = ["timers.target"];
        timerConfig = {
          OnCalendar = cfg.schedule;
          Persistent = true;
          RandomizedDelaySec = "5m";
          AccuracySec = "1m";
          Unit = "neo-auto-update.service";
        };
      };

      # homeserver runs neo update/activate and optional GC as this user.
      # (web UI store repair / systemd-run / rm live on neo.services.neo)
      security.sudo.extraRules = lib.neo.mkSudoExtraRules {
        users = ["homeserver"];
        commands = [
          # neo activate → sudo nixos-rebuild switch|boot
          {
            package = pkgs.nixos-rebuild;
            name = "nixos-rebuild";
          }
          # neo activate → systemctl reset-failed/stop of rebuild unit;
          # delayed boot-apply → systemctl reboot (via systemd-run unit)
          {
            package = pkgs.systemd;
            name = "systemctl";
          }
          # schedule delayed host cycle after boot activate
          {
            package = pkgs.systemd;
            name = "systemd-run";
          }
          # scheduled GC before update (`sudo nix-collect-garbage --delete-older-than …`)
          {
            package = pkgs.nix;
            name = "nix-collect-garbage";
          }
        ];
      };
    };
  };
}
