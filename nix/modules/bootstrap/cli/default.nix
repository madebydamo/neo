{...}: {
  flake.modules.nixos.cli = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.neo.homeserverConfig;
    settingsPath = ../../../../settings.toml;
    settingsContent =
      if builtins.pathExists settingsPath
      then builtins.readFile settingsPath
      else ''
        # settings.toml - from current activation
        # [device]
        # hostname = "homeserver"
        # grubDevice = "/dev/sda"
        # rootFsDevice = "/dev/sda1"
        # rootFsType = "ext4"
      '';
    neo-cli = pkgs.writeShellScriptBin "neo" ''
      set -euo pipefail
      NIX_CMD="${pkgs.nix}/bin/nix"
      COMMAND="''${1:-help}"
      shift || true
      CONFIG_PATH="${cfg.configPath}"
      REBUILD_BRANCH_FORMAT="${cfg.rebuildBranchFormat}"
      case "$COMMAND" in
        generate-hardware)
          mkdir -p "''${CONFIG_PATH}"
          cd "''${CONFIG_PATH}"
          ${pkgs.nixos-install-tools}/bin/nixos-generate-config --show-hardware-config > hardware-configuration.nix
          echo "Generated hardware-configuration.nix"
          ;;
        paste-settings)
          cd "''${CONFIG_PATH}"
          cp -f /etc/neo/settings.toml settings.toml 2>/dev/null || true
          echo "Pasted settings.toml from current activation to configuration folder"
          ;;

      init)
        mkdir -p "''${CONFIG_PATH}"
        cd "''${CONFIG_PATH}"
        ${pkgs.git}/bin/git config --global user.name "${cfg.gitUserName}"
        ${pkgs.git}/bin/git config --global user.email "${cfg.gitUserEmail}"
        ${pkgs.git}/bin/git config --global init.defaultBranch "${cfg.defaultBranch}"
        ${pkgs.git}/bin/git config --global --add safe.directory "''${CONFIG_PATH}" || true
        if [ -n "${cfg.repoUrl}" ] && [ "${cfg.bootstrapMethod}" = "clone" ]; then
          ${pkgs.git}/bin/git clone --depth 1 "${cfg.repoUrl}" . || ${pkgs.git}/bin/git clone "${cfg.repoUrl}" .
        else
          "$NIX_CMD" --extra-experimental-features 'nix-command flakes' flake init -t github:madebydamo/neo#homeserver
          ${pkgs.git}/bin/git init
          if [ -n "${cfg.repoUrl}" ]; then
            ${pkgs.git}/bin/git remote add origin "${cfg.repoUrl}"
          fi
        fi

        neo generate-hardware
        neo paste-settings
        ${pkgs.git}/bin/git add .
        "$NIX_CMD" --extra-experimental-features 'nix-command flakes' run .#write-flake || true
        ${pkgs.git}/bin/git add .
        ${pkgs.git}/bin/git commit -m "Initial commit from neo init" || true
        echo "Repo initialized at ''${CONFIG_PATH}"
        ;;




        update)
          cd "''${CONFIG_PATH}"
          "$NIX_CMD" --extra-experimental-features 'nix-command flakes' run .#write-flake
          "$NIX_CMD" --extra-experimental-features 'nix-command flakes' flake update
          echo "Flake updated"
          ;;


        activate)
          cd "''${CONFIG_PATH}"
          "$NIX_CMD" --extra-experimental-features 'nix-command flakes' run .#write-flake
          "$NIX_CMD" --extra-experimental-features 'nix-command flakes' build .#nixosConfigurations.neo.config.system.build.toplevel
          BRANCH=$(date +"$REBUILD_BRANCH_FORMAT")
          ${pkgs.git}/bin/git switch -C "$BRANCH" || true
          ${pkgs.git}/bin/git add .
          ${pkgs.git}/bin/git diff --staged --quiet || ${pkgs.git}/bin/git commit -m "Rebuild: $BRANCH" || true
          /run/current-system/sw/bin/nixos-rebuild switch --flake .#neo
          echo "Activated using branch $BRANCH"
          ;;
        nuke)
          read -p "Nuke ''${CONFIG_PATH}? This will delete the entire folder (y/N): " -n 1 -r
          echo
          if [[ "$REPLY" =~ ^[Yy]$ ]]; then
            rm -rf "''${CONFIG_PATH}"
            echo "Nuked ''${CONFIG_PATH}"
          else
            echo "Cancelled"
          fi
          ;;
              help|--help)
                echo "neo <command>"
                echo "  generate-hardware"
                echo "  paste-settings"
                echo "  init"
                echo "  update"
                echo "  activate"
                echo "  nuke"
                ;;
              *)
                echo "Unknown command ''${COMMAND}. Use neo help"
                exit 1
                ;;
            esac

    '';
  in {
    config = {
      environment.systemPackages = [
        neo-cli
        pkgs.git
        pkgs.nix
        pkgs.nixos-install-tools
      ];
      environment.etc."neo/settings.toml".text = settingsContent;
    };
  };
}
