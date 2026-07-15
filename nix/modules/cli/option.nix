# Neo CLI configuration: shared options + local/server profiles (configPath differs by default).
{...}: {
  flake.modules.nixos.cli-option = {
    config,
    lib,
    ...
  }: let
    inherit (lib) types;
    inherit (lib.neo) mkOption;
    profileConfigPath = {
      default,
      description,
      rank,
    }:
      mkOption {
        type = types.submodule {
          options = {
            configPath = mkOption {
              type = types.str;
              inherit default description;
              rank = 0;
            };
          };
        };
        default = {};
        description = "Profile-specific CLI settings (only configPath by default)";
        inherit rank;
      };
  in {
    options.neo.neo-cli = mkOption {
      type = types.submodule (
        {...}: {
          options = {
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
            local = profileConfigPath {
              default = "./build";
              description = "Config repo path when running the CLI off-box (laptop / nix run)";
              rank = 90;
            };
            server = profileConfigPath {
              default = "${config.neo.core.volumes.appdata}/configuration";
              description = "Config repo path on the homeserver (system-updater and on-box neo always use this profile)";
              rank = 100;
            };
          };
        }
      );
      default = {};
      description = "Neo CLI configuration (shared keys + local/server profiles for configPath)";
    };
  };
}
