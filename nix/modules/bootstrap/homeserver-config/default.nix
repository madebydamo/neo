{...}: {
  flake.modules.nixos.homeserver-config = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.homeserverConfig;
  in
    lib.mkIf cfg.enabled {
      system.activationScripts.homeserver-config-dirs = lib.neo.mkActivationScriptForDir config {
        dirPath = cfg.configPath;
        mode = "0755";
      };
      system.activationScripts.homeserver-config-safe-directory = lib.mkIf cfg.safeDirectoryEnable (
        lib.stringAfter ["symlinks"] ''
          ${lib.getBin pkgs.git}/bin/git config --system --add safe.directory ${cfg.configPath}
        ''
      );
      # system.activationScripts.homeserver-config-plugins = lib.mkIf cfg.pluginGeneratorEnable (
      #   lib.stringAfter ["homeserver-config-dirs"] ''
      #             cat > ${cfg.configPath}/plugins.toml << EOF
      #     ${pluginsTOML}
      #     EOF
      #   ''
      # );
      systemd.services.neo-homeserver-config-bootstrap =
        lib.mkIf (cfg.bootstrapEnable && cfg.repoUrl != null)
        {
          description = "Bootstrap homeserver config git repo";
          wantedBy = ["multi-user.target"];
          before = ["multi-user.target"];
          path = with pkgs; [
            git
            nix
          ];
          serviceConfig = {
            Type = "oneshot";
            RemainAfterExit = false;
          };

          script = ''
            set -e
            CONFIG_PATH="${cfg.configPath}"
             if [ ! -d "$CONFIG_PATH/.git" ]; then
               cd "$CONFIG_PATH"
               nix flake init -t github:madebydamo/neo#homeserver
               ${lib.getBin pkgs.git}/bin/git init
               ${lib.getBin pkgs.git}/bin/git remote add origin ${cfg.repoUrl}
               ${lib.getBin pkgs.git}/bin/git config user.name "Neo Bootstrap"
               ${lib.getBin pkgs.git}/bin/git config user.email "bootstrap@neo"
               ${lib.getBin pkgs.git}/bin/git add .
               ${lib.getBin pkgs.git}/bin/git commit -m "Initial bootstrap from Neo template"
               ${lib.getBin pkgs.git}/bin/git branch -M main
               ${lib.getBin pkgs.git}/bin/git push -u origin main || true
            fi

          '';
          serviceConfig.User = cfg.bootstrapUser;
        };
      systemd.services.neo-homeserver-config-rebuild = lib.mkIf cfg.rebuildEnable {
        description = "Rebuild homeserver config to timestamp branch";
        wants = ["neo-homeserver-config-bootstrap.service"];
        after = ["neo-homeserver-config-bootstrap.service"];
        path = [
          pkgs.git
          pkgs.coreutils
          pkgs.btrfs-progs
        ];
        serviceConfig = {
          Type = "oneshot";
          RemainAfterExit = false;
        };
        script = ''
          set -e
          cd ${cfg.configPath}
          ${lib.getBin pkgs.git}/bin/git pull origin main
          BRANCH=\$(date +${cfg.rebuildBranchFormat})
          ${lib.getBin pkgs.git}/bin/git switch -C \$BRANCH
          /run/current-system/sw/bin/nixos-rebuild switch --flake .#homeserver
          ${lib.optionalString (cfg.rebuildBtrfsSubvol != null) ''
            ${lib.getBin pkgs.btrfs-progs}/bin/btrfs subvolume snapshot -r ${cfg.rebuildBtrfsSubvol} ${cfg.rebuildBtrfsSubvol}-\$BRANCH
          ''}
          ${lib.getBin pkgs.git}/bin/git add .
          ${lib.getBin pkgs.git}/bin/git diff --staged --quiet || ${lib.getBin pkgs.git}/bin/git commit -m "Rebuild: \$BRANCH"
          ${lib.getBin pkgs.git}/bin/git push origin \$BRANCH
        '';
        serviceConfig.User = "root";
      };
      systemd.timers.neo-homeserver-config-rebuild = lib.mkIf cfg.rebuildEnable {
        wantedBy = ["timers.target"];
        timerConfig.OnCalendar = cfg.rebuildTimer;
        timerConfig.Persistent = true;
      };
    };
}
