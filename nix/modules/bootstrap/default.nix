{self, ...}: {
  flake.modules.nixos.bootstrap = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo."neo-service";
    neo = self.packages.${pkgs.stdenv.hostPlatform.system}.neo;
    path = [
      neo
      pkgs.git
      pkgs.nix
      pkgs.nixos-rebuild
      pkgs.nixos-install-tools
      pkgs.coreutils
      pkgs.bash
      pkgs.sudo
    ];
    environment = {
      NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
      SUDO_BINARY_PATH = "/run/wrappers/bin/sudo";
    };
  in {
    systemd.services.neo-bootstrap = lib.mkIf cfg.bootstrapEnabled {
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
        dirPath = cfg.configPath;
        mode = "0755";
      };
      script = ''
        ${neo}/bin/neo init
      '';
    };

    systemd.services.neo-auto-update = lib.mkIf (cfg.autoUpdateEnabled && cfg.bootstrapEnabled) {
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
        lib.optionalString (cfg.garbageCollectOlderThen == null) ''
          sudo nix-collect-garbage --delete-older-than ${cfg.garbageCollectOlderThen} || true
        ''
        + ''
          ${neo}/bin/neo update && ${neo}/bin/neo activate
        '';
    };

    systemd.timers.neo-auto-update = lib.mkIf cfg.autoUpdateEnabled {
      wantedBy = ["timers.target"];
      timerConfig = {
        OnCalendar = cfg.autoUpdateTimer;
        Persistent = true;
        RandomizedDelaySec = "5m";
        AccuracySec = "1m";
        Unit = "neo-auto-update.service";
      };
    };
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
}
