# Neo CLI configuration: shared options + local/server profiles.
# configPath, neoInput, and template differ by profile. The server profile
# defaults neoInput to github:madebydamo/neo.
{...}: {
  flake.modules.nixos.cli-option = {
    config,
    lib,
    ...
  }: let
    inherit (lib) types;
    inherit (lib.neo) mkOption;
    profileSettings = {
      configPathDefault,
      configPathDescription,
      rank,
    }:
      mkOption {
        type = types.submodule {
          options = {
            configPath = mkOption {
              type = types.str;
              default = configPathDefault;
              description = configPathDescription;
              rank = 0;
            };
            neoInput = mkOption {
              type = types.str;
              default = "github:madebydamo/neo";
              description = "Nix flake input for neo on this profile. The server default is github:madebydamo/neo. A laptop checkout (git+file: or path:) belongs on the local profile.";
              rank = 10;
            };
            template = mkOption {
              type = types.str;
              default = "";
              description = "Template for nix flake init -t on this profile. Empty derives it from neoInput (#homeserver). git+file and path inputs use the directory.";
              rank = 20;
            };
          };
        };
        default = {};
        description = "Profile-specific CLI settings (configPath, neoInput, template)";
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
              description = "Fallback Nix input for neo when a profile does not set neoInput. The server profile ignores a local filesystem path here.";
              rank = 20;
            };
            template = mkOption {
              type = types.str;
              default = "github:madebydamo/neo#homeserver";
              description = "Fallback template when a profile sets neither template nor neoInput. The server profile ignores a local filesystem path here.";
              rank = 30;
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
            local = profileSettings {
              configPathDefault = "./build";
              configPathDescription = "Config repo path when running the CLI off-box (laptop / nix run)";
              rank = 90;
            };
            server = profileSettings {
              configPathDefault = "${config.neo.core.volumes.appdata}/configuration";
              configPathDescription = "Config repo path on the homeserver (neo-bootstrap and on-box neo always use this profile)";
              rank = 100;
            };
          };
        }
      );
      default = {};
      description = "Neo CLI configuration (shared keys + local/server profiles for configPath, neoInput, and template)";
    };
  };
}
