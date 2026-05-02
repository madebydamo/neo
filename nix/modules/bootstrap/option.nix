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
      mdDoc
      ;
  in {
    options.neo.nixos = mkOption {
      type = types.submodule (
        {...}: {
          options = {
            enabled = mkEnableOption (mdDoc "Enable nixos configuration bootstrap");
            repoUrl = mkOption {
              type = types.str;
              default = "";
              description = mdDoc "Git repository URL for the nixos configuration";
            };
            configPath = mkOption {
              type = types.str;
              default = "${config.neo.volumes.appdata}/configuration";
              description = mdDoc "Path to the nixos configuration repository";
            };
            neoInput = mkOption {
              type = types.str;
              default = "github:madebydamo/neo";
              description = mdDoc "Nix input for neo";
            };
            template = mkOption {
              type = types.str;
              default = "github:madebydamo/neo#homeserver";
              description = mdDoc "Base template to use for initializing configuration";
            };
            bootstrapEnabled = mkEnableOption (mdDoc "Bootstrap the git repository if .git is missing");
            bootstrapMethod = mkOption {
              type = types.enum [
                "template"
                "clone"
              ];
              default = "template";
              description = mdDoc "Bootstrap method: 'template' uses flake init, 'clone' uses git clone from repoUrl";
            };
            gitUserName = mkOption {
              type = types.str;
              default = "Neo Bootstrap";
              description = mdDoc "Git user.name used for initial commits";
            };
            gitUserEmail = mkOption {
              type = types.str;
              default = "neo@local";
              description = mdDoc "Git user.email used for initial commits";
            };
            defaultBranch = mkOption {
              type = types.str;
              default = "master";
              description = mdDoc "Default branch name for git init";
            };
            pluginGeneratorPlugins = mkOption {
              type = types.listOf types.str;
              default = [];
              description = mdDoc "List of plugins (kept for future use)";
            };
            autoUpdateEnabled = mkEnableOption (mdDoc "Enable auto-update timer and service");
            autoUpdateTimer = mkOption {
              type = types.str;
              default = "*-*-* 04:00:00";
              description = mdDoc "systemd timer OnCalendar value";
            };
            rebuildBranchFormat = mkOption {
              type = types.str;
              default = "%Y%m%d-%H%M%S";
              description = mdDoc "printf format for branch name";
            };
          };
        }
      );
      default = {};
      description = mdDoc "Nixos bootstrap configuration";
    };
    options.neo.cli = mkOption {
      type = types.submodule (
        {...}: {
          options = {
            configPath = mkOption {
              type = types.str;
              default = "./build";
              description = mdDoc "Path to the nixos configuration repository (for CLI)";
            };
            repoUrl = mkOption {
              type = types.str;
              default = "";
              description = mdDoc "Git repository URL for the nixos configuration";
            };
            neoInput = mkOption {
              type = types.str;
              default = "github:madebydamo/neo";
              description = mdDoc "Nix input for neo";
            };
            template = mkOption {
              type = types.str;
              default = "github:madebydamo/neo#homeserver";
              description = mdDoc "Base template to use for initializing configuration";
            };
            bootstrapMethod = mkOption {
              type = types.enum [
                "template"
                "clone"
              ];
              default = "template";
              description = mdDoc "Bootstrap method: 'template' uses flake init, 'clone' uses git clone from repoUrl";
            };
            gitUserName = mkOption {
              type = types.str;
              default = "Neo Bootstrap";
              description = mdDoc "Git user.name used for initial commits";
            };
            gitUserEmail = mkOption {
              type = types.str;
              default = "neo@local";
              description = mdDoc "Git user.email used for initial commits";
            };
            defaultBranch = mkOption {
              type = types.str;
              default = "master";
              description = mdDoc "Default branch name for git init";
            };
            rebuildBranchFormat = mkOption {
              type = types.str;
              default = "%Y%m%d-%H%M%S";
              description = mdDoc "printf format for branch name";
            };
          };
        }
      );
      default = {};
      description = mdDoc "Neo CLI package configuration";
    };
  };
}
