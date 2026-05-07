{...}: {
  flake.modules.nixos.bootstrap-option = {
    config,
    lib,
    ...
  }: let
    inherit
      (lib)
      types
      mkOption
      mkEnableOption
      ;
  in {
    options.neo.nixos = mkOption {
      type = types.submodule (
        {...}: {
          options = {
            enabled = mkEnableOption "Enable nixos configuration bootstrap";
            repoUrl = mkOption {
              type = types.str;
              default = "";
              description = "Git repository URL for the nixos configuration";
            };
            configPath = mkOption {
              type = types.str;
              default = "${config.neo.volumes.appdata}/configuration";
              description = "Path to the nixos configuration repository";
            };
            neoInput = mkOption {
              type = types.str;
              default = "github:madebydamo/neo";
              description = "Nix input for neo";
            };
            template = mkOption {
              type = types.str;
              default = "github:madebydamo/neo#homeserver";
              description = "Base template to use for initializing configuration";
            };
            bootstrapEnabled = mkEnableOption "Bootstrap the git repository if .git is missing";
            bootstrapMethod = mkOption {
              type = types.enum [
                "template"
                "clone"
              ];
              default = "template";
              description = "Bootstrap method: 'template' uses flake init, 'clone' uses git clone from repoUrl";
            };
            gitUserName = mkOption {
              type = types.str;
              default = "Neo Bootstrap";
              description = "Git user.name used for initial commits";
            };
            gitUserEmail = mkOption {
              type = types.str;
              default = "neo@local";
              description = "Git user.email used for initial commits";
            };
            defaultBranch = mkOption {
              type = types.str;
              default = "master";
              description = "Default branch name for git init";
            };
            pluginGeneratorPlugins = mkOption {
              type = types.listOf types.str;
              default = [];
              description = "List of plugins (kept for future use)";
            };
            autoUpdateEnabled = mkEnableOption "Enable auto-update timer and service";
            autoUpdateTimer = mkOption {
              type = types.str;
              default = "*-*-* 04:00:00";
              description = "systemd timer OnCalendar value";
            };
            rebuildBranchFormat = mkOption {
              type = types.str;
              default = "%Y%m%d-%H%M%S";
              description = "printf format for branch name";
            };
          };
        }
      );
      default = {};
      description = "Nixos bootstrap configuration";
    };
    options.neo.cli = mkOption {
      type = types.submodule (
        {...}: {
          options = {
            configPath = mkOption {
              type = types.str;
              default = "./build";
              description = "Path to the nixos configuration repository (for CLI)";
            };
            repoUrl = mkOption {
              type = types.str;
              default = "";
              description = "Git repository URL for the nixos configuration";
            };
            neoInput = mkOption {
              type = types.str;
              default = "github:madebydamo/neo";
              description = "Nix input for neo";
            };
            template = mkOption {
              type = types.str;
              default = "github:madebydamo/neo#homeserver";
              description = "Base template to use for initializing configuration";
            };
            bootstrapMethod = mkOption {
              type = types.enum [
                "template"
                "clone"
              ];
              default = "template";
              description = "Bootstrap method: 'template' uses flake init, 'clone' uses git clone from repoUrl";
            };
            gitUserName = mkOption {
              type = types.str;
              default = "Neo Bootstrap";
              description = "Git user.name used for initial commits";
            };
            gitUserEmail = mkOption {
              type = types.str;
              default = "neo@local";
              description = "Git user.email used for initial commits";
            };
            defaultBranch = mkOption {
              type = types.str;
              default = "master";
              description = "Default branch name for git init";
            };
            rebuildBranchFormat = mkOption {
              type = types.str;
              default = "%Y%m%d-%H%M%S";
              description = "printf format for branch name";
            };
          };
        }
      );
      default = {};
      description = "Neo CLI package configuration";
    };
  };
}
