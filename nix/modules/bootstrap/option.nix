{...}: {
  flake.modules.nixos.bootstrap-option = {
    config,
    lib,
    ...
  }: let
    inherit (lib) types;
    inherit (lib.neo) mkOption mkEnableOption;
  in {
    options.neo.neo-service = mkOption {
      type = types.submodule (
        {...}: {
          options = {
            enabled = mkEnableOption "Enable nixos configuration bootstrap" {rank = 0;};
            repoUrl = mkOption {
              type = types.str;
              default = "";
              description = "Git repository URL for the nixos configuration";
              rank = 10;
            };
            configPath = mkOption {
              type = types.str;
              default = "${config.neo.core.volumes.appdata}/configuration";
              description = "Path to the nixos configuration repository";
              rank = 20;
            };
            neoInput = mkOption {
              type = types.str;
              default = "github:madebydamo/neo";
              description = "Nix input for neo";
              rank = 30;
            };
            template = mkOption {
              type = types.str;
              default = "github:madebydamo/neo#homeserver";
              description = "Base template to use for initializing configuration";
              rank = 40;
            };
            bootstrapEnabled = mkEnableOption "Bootstrap the git repository if .git is missing" {rank = 50;};
            bootstrapMethod = mkOption {
              type = types.enum [
                "template"
                "clone"
              ];
              default = "template";
              description = "Bootstrap method: 'template' uses flake init, 'clone' uses git clone from repoUrl";
              rank = 60;
            };
            gitUserName = mkOption {
              type = types.str;
              default = "Neo Bootstrap";
              description = "Git user.name used for initial commits";
              rank = 70;
            };
            gitUserEmail = mkOption {
              type = types.str;
              default = "neo@local";
              description = "Git user.email used for initial commits";
              rank = 80;
            };
            defaultBranch = mkOption {
              type = types.str;
              default = "master";
              description = "Default branch name for git init";
              rank = 90;
            };
            plugins = mkOption {
              type = types.listOf types.str;
              default = [];
              description = "List of plugins";
              rank = 100;
            };
            autoUpdateEnabled = mkEnableOption "Enable auto-update timer and service" {rank = 110;};
            autoUpdateTimer = mkOption {
              type = types.str;
              default = "*-*-* 04:00:00";
              description = "systemd timer OnCalendar value";
              rank = 120;
            };
            rebuildBranchFormat = mkOption {
              type = types.str;
              default = "%Y%m%d-%H%M%S";
              description = "printf format for branch name";
              rank = 130;
            };
          };
        }
      );
      default = {};
      description = "Nixos bootstrap configuration";
    };
    options.neo.neo-cli = mkOption {
      type = types.submodule (
        {...}: {
          options = {
            configPath = mkOption {
              type = types.str;
              default = "./build";
              description = "Path to the nixos configuration repository (for CLI)";
              rank = 0;
            };
            repoUrl = mkOption {
              type = types.str;
              default = "";
              description = "Git repository URL for the nixos configuration";
              rank = 10;
            };
            neoInput = mkOption {
              type = types.str;
              default = "github:madebydamo/neo";
              description = "Nix input for neo";
              rank = 20;
            };
            template = mkOption {
              type = types.str;
              default = "github:madebydamo/neo#homeserver";
              description = "Base template to use for initializing configuration";
              rank = 30;
            };
            bootstrapMethod = mkOption {
              type = types.enum [
                "template"
                "clone"
              ];
              default = "template";
              description = "Bootstrap method: 'template' uses flake init, 'clone' uses git clone from repoUrl";
              rank = 40;
            };
            gitUserName = mkOption {
              type = types.str;
              default = "Neo Bootstrap";
              description = "Git user.name used for initial commits";
              rank = 50;
            };
            gitUserEmail = mkOption {
              type = types.str;
              default = "neo@local";
              description = "Git user.email used for initial commits";
              rank = 60;
            };
            defaultBranch = mkOption {
              type = types.str;
              default = "master";
              description = "Default branch name for git init";
              rank = 70;
            };
            rebuildBranchFormat = mkOption {
              type = types.str;
              default = "%Y%m%d-%H%M%S";
              description = "printf format for branch name";
              rank = 80;
            };
          };
        }
      );
      default = {};
      description = "Neo CLI package configuration";
    };
  };
}
