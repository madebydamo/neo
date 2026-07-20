# System auto-updater: bootstrap + scheduled neo update/activate (replaces neo-service auto-update).
{self, ...}: {
  flake.modules.nixos.system-updater = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.services.system-updater;
    # Always the server profile — never local/laptop paths.
    serverCfg = config.neo.neo-cli.server;
    neo = self.packages.${pkgs.stdenv.hostPlatform.system}.neo;
    path = [
      neo
      pkgs.git
      pkgs.nix
      pkgs.nixos-rebuild
      pkgs.nixos-install-tools
      pkgs.coreutils
      pkgs.bash
    ];
    environment = {
      NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
      SUDO_BINARY_PATH = "/run/wrappers/bin/sudo";
    };
  in {
    config = lib.mkIf cfg.enabled {
      # Keep build outputs of live derivations so build-time deps (e.g. crane/cargo
      # crate builds) are not recompiled after each scheduled GC.
      nix.settings.keep-outputs = lib.mkIf (cfg.garbageCollectOlderThen != null) true;

      systemd.services.neo-bootstrap = {
        description = "Bootstrap nixos config git repo";
        after = ["network-online.target"];
        wants = ["network-online.target"];
        wantedBy = ["multi-user.target"];
        before = ["multi-user.target"];
        inherit path;
        inherit environment;
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
          User = "homeserver";
          Group = "homeserver";
        };
        preStart = lib.neo.mkActivationScriptForDir config {
          dirPath = serverCfg.configPath;
          mode = "0755";
        };
        script = ''
          ${neo}/bin/neo --profile server init
        '';
      };

      systemd.services.neo-auto-update = {
        description = "Auto update and activate nixos config with neo";
        wants = ["neo-bootstrap.service"];
        after = ["neo-bootstrap.service"];
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
        script =
          lib.optionalString (cfg.garbageCollectOlderThen != null) ''
            /run/wrappers/bin/sudo nix-collect-garbage --delete-older-than ${cfg.garbageCollectOlderThen} || true
          ''
          + ''
            ${neo}/bin/neo --profile server update && ${neo}/bin/neo --profile server activate
          '';
      };

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

      # Allow homeserver to run rebuild/GC/systemctl (web UI activate, manual neo).
      security.sudo.extraRules = [
        {
          users = ["homeserver"];
          commands = [
            {
              command = "${pkgs.nixos-rebuild}/bin/nixos-rebuild";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "/run/current-system/sw/bin/nixos-rebuild";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "/nix/store/*-nixos-rebuild-*/bin/nixos-rebuild";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "${pkgs.nix}/bin/nix-collect-garbage";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "/run/current-system/sw/bin/nix-collect-garbage";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "/nix/store/*-nix-*/bin/nix-collect-garbage";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            # Also used by neo-web store repair.
            {
              command = "${pkgs.nix}/bin/nix-store";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "/run/current-system/sw/bin/nix-store";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "/nix/store/*-nix-*/bin/nix-store";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "/run/current-system/sw/bin/systemctl";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "${pkgs.systemd}/bin/systemctl";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "/nix/store/*-systemd-*/bin/systemctl";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "/run/current-system/sw/bin/systemd-run";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
            {
              command = "/nix/store/*-systemd-*/bin/systemd-run";
              options = [
                "NOPASSWD"
                "SETENV"
              ];
            }
          ];
        }
      ];
    };
  };
}
