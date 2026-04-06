{self, ...}: {
  flake.modules.nixos.bootstrap = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.nixos;
  in
    lib.mkIf cfg.enabled {
      system.activationScripts.nixos-config-dirs = lib.neo.mkActivationScriptForDir config {
        dirPath = cfg.configPath;
        mode = "0755";
      };
      # always do safe.directory
      system.activationScripts.nixos-config-safe-directory = lib.stringAfter ["nixos-config-dirs"] ''
        ${lib.getBin pkgs.git}/bin/git config --system --add safe.directory ${cfg.configPath}
      '';

      systemd.services.neo-bootstrap = lib.mkIf cfg.bootstrapEnabled {
        description = "Bootstrap nixos config git repo";
        wantedBy = ["multi-user.target"];
        before = ["multi-user.target"];
        path = [
          self.packages.${pkgs.system}.neo
          pkgs.git
          pkgs.nix
          pkgs.coreutils
        ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = true;
        };
        script = ''
          set -e
          CONFIG_PATH="${cfg.configPath}"
          if [ ! -d "$CONFIG_PATH/.git" ]; then
            echo "Bootstrapping using neo init at $CONFIG_PATH..."
            neo init
          else
            echo "✓ Git repository already exists at $CONFIG_PATH"
          fi
        '';
      };

      systemd.services.neo-auto-update = lib.mkIf cfg.autoUpdateEnabled {
        description = "Auto update and activate nixos config with neo";
        wants = ["neo-bootstrap.service"];
        after = ["neo-bootstrap.service"];
        path = [
          self.packages.${pkgs.system}.neo
          pkgs.git
          pkgs.coreutils
        ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = false;
        };
        script = ''
          set -e
          cd "${cfg.configPath}"
          neo update && neo activate
        '';
      };

      systemd.timers.neo-auto-update = lib.mkIf cfg.autoUpdateEnabled {
        wantedBy = ["timers.target"];
        timerConfig.OnCalendar = cfg.autoUpdateTimer;
        timerConfig.Persistent = true;
      };
    };
}
