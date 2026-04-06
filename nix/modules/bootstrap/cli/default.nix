{
  inputs,
  self,
  pkgs,
  ...
}: let
  cfg = self.nixosConfigurations.homeserver.config.neo.homeserverConfig;
in {
  perSystem = {
    config,
    pkgs,
    ...
  }: let
    settingsPath = ../../../../settings.toml;
    settingsContent =
      if builtins.pathExists settingsPath
      then builtins.readFile settingsPath
      else ''
        # settings.toml - from current activation
        # [device]
        # hostname = "homeserver"
      '';
    defaultSettings = pkgs.writeText "default-settings.toml" settingsContent;
  in {
    packages.neo = pkgs.writeShellScriptBin "neo" ''
      set -euo pipefail
      NIX_CMD="${pkgs.nix}/bin/nix"
      COMMAND="''${1:-help}"
      shift || true
      CONFIG_PATH="${cfg.configPath}"
      REBUILD_BRANCH_FORMAT="${cfg.rebuildBranchFormat}"
      FLAKE_INIT_TEMPLATE="${cfg.template}"

      case "$COMMAND" in
        generate-hardware)
          mkdir -p "$CONFIG_PATH"
          (cd "$CONFIG_PATH" && ${pkgs.nixos-install-tools}/bin/nixos-generate-config --show-hardware-config > hardware-configuration.nix)
          echo "Generated hardware-configuration.nix in $CONFIG_PATH"
          ;;
        paste-settings)
          (cd "$CONFIG_PATH" && cp -f ${defaultSettings} settings.toml 2>/dev/null)
          echo "Pasted settings.toml from current activation to $CONFIG_PATH"
          ;;

        init)
          mkdir -p "$CONFIG_PATH"
          (cd "$CONFIG_PATH" && {
            # ── Smart init: handle existing folder gracefully ─────────────────────
            if [ -d .git ]; then
              echo "✓ Git repository already exists at $CONFIG_PATH"
              echo "  (re-running setup steps — safe even if the worktree is dirty)"
            else
              if [ -n "$(ls -A . 2>/dev/null | grep -v '^\.' | head -1)" ]; then
                echo "❌ Error: $CONFIG_PATH is not empty and is not a git repository."
                echo "   Please remove the files first or use a different directory."
                exit 1
              fi

              echo "→ Initializing new repository at $CONFIG_PATH..."
            fi
            if [ ! -f flake.nix ]; then
              if [ -n "${cfg.repoUrl}" ] && [ "${cfg.bootstrapMethod}" = "clone" ]; then
                ${pkgs.git}/bin/git clone "${cfg.repoUrl}" .
              else
                "$NIX_CMD" --extra-experimental-features 'nix-command flakes' flake init -t "$FLAKE_INIT_TEMPLATE"
                ${pkgs.git}/bin/git init
                if [ -n "${cfg.repoUrl}" ]; then
                  ${pkgs.git}/bin/git remote add origin "${cfg.repoUrl}"
                fi
              fi
            fi

            ${pkgs.git}/bin/git config user.name "${cfg.gitUserName}"
            ${pkgs.git}/bin/git config user.email "${cfg.gitUserEmail}"
            ${pkgs.git}/bin/git config init.defaultBranch "${cfg.defaultBranch}"
          })

          echo "→ Generating hardware configuration..."
          "$0" generate-hardware

          echo "→ Pasting settings..."
          "$0" paste-settings

          echo "→ Update inputs..."
          (cd "$CONFIG_PATH" && ${pkgs.git}/bin/git add .)
          "$0" update-inputs

          (cd "$CONFIG_PATH" && {
            ${pkgs.git}/bin/git add .

            if ${pkgs.git}/bin/git diff --cached --quiet 2>/dev/null; then
              echo "✓ No changes to commit (everything is up-to-date)"
            else
              ${pkgs.git}/bin/git commit -m "Update from neo init"
            fi
          })
          echo "Repository ready at $CONFIG_PATH"
          ;;

        update)
          (cd "$CONFIG_PATH" && "$NIX_CMD" --extra-experimental-features 'nix-command flakes' flake update)
          echo "Flake updated in $CONFIG_PATH"
          ;;

        update-inputs)
          (cd "$CONFIG_PATH" && "$NIX_CMD" --extra-experimental-features 'nix-command flakes' run .#write-flake)
          echo "Flake updated in $CONFIG_PATH"
          ;;

        activate)
          (cd "$CONFIG_PATH" && {
            "$NIX_CMD" --extra-experimental-features 'nix-command flakes' run .#write-flake
            "$NIX_CMD" --extra-experimental-features 'nix-command flakes' build .#nixosConfigurations.neo.config.system.build.toplevel
            BRANCH=$(date +"$REBUILD_BRANCH_FORMAT")
            ${pkgs.git}/bin/git switch -C "$BRANCH" || true
            ${pkgs.git}/bin/git add .
            ${pkgs.git}/bin/git diff --staged --quiet || ${pkgs.git}/bin/git commit -m "Rebuild: $BRANCH" || true
            /run/current-system/sw/bin/nixos-rebuild switch --flake .#neo
            echo "Activated using branch $BRANCH"
          })
          ;;
        build)
          (cd "$CONFIG_PATH" && {
            "$NIX_CMD" --extra-experimental-features 'nix-command flakes' run .#write-flake
            # for switching
            "$NIX_CMD" --extra-experimental-features 'nix-command flakes' build .#nixosConfigurations.neo.config.system.build.toplevel
            # for vm
            "$NIX_CMD" --extra-experimental-features 'nix-command flakes' build .#nixosConfigurations.vm.config.system.build.vm
            echo "Built configuration"
          })
          ;;

        nuke)
          rm -rf "$CONFIG_PATH"/*
          echo "Nuked $CONFIG_PATH"
          ;;
        help|--help)
          echo "neo <command>"
          echo "  generate-hardware"
          echo "  paste-settings"
          echo "  init"
          echo "  update"
          echo "  update-inputs"
          echo "  build"
          echo "  activate"
          echo "  nuke"
          ;;
        *)
          echo "Unknown command $COMMAND. Use neo help"
          exit 1
          ;;
      esac
    '';
  };
  flake.modules.nixos.cli = {
    lib,
    pkgs,
    ...
  }: {
    config = {
      environment.systemPackages = [
        self.packages."x86_64-linux".neo
        pkgs.git
        pkgs.nix
        pkgs.nixos-install-tools
        pkgs.coreutils
      ];
    };
  };
}
