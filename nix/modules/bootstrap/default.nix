{self, ...}: {
  flake.modules.nixos.bootstrap = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.nixos;
    neo = self.packages.${pkgs.system}.neo;
    path = [
      neo
      pkgs.git
      pkgs.nixos-rebuild
      pkgs.coreutils
      pkgs.bash
    ];
    environment = {
      NEO_SETTINGS = "${cfg.configPath}/settings.toml";
      NEO_SECTION = "nixos";
      NIX_BINARY_PATH = "${pkgs.nix}/bin/nix";
      SUDO_BINARY_PATH = "/run/wrappers/bin/sudo";
    };
  in {
    systemd.services.neo-bootstrap = lib.mkIf cfg.bootstrapEnabled {
      description = "Bootstrap nixos config git repo";
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
        CONFIG_PATH="${cfg.configPath}"
        if [ ! -d "$CONFIG_PATH/.git" ]; then
          echo "Bootstrapping using neo init at $CONFIG_PATH..."
          ${neo}/bin/neo init
        else
          echo "✓ Git repository already exists at $CONFIG_PATH"
        fi
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
      script = ''
        # cd "${cfg.configPath}"
        ${neo}/bin/neo update && ${neo}/bin/neo activate
      '';
    };

    systemd.timers.neo-auto-update = lib.mkIf cfg.autoUpdateEnabled {
      wantedBy = ["timers.target"];
      timerConfig.OnCalendar = cfg.autoUpdateTimer;
      timerConfig.Persistent = true;
    };
    security.sudo.extraRules = [
      {
        users = ["homeserver"];
        commands = [
          {
            command = "${pkgs.nixos-rebuild}bin/nixos-rebuild";
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
