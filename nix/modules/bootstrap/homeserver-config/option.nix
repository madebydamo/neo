{...}: {
  flake.modules.nixos.homeserver-config-option = {
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
    options.neo.homeserverConfig = mkOption {
      type = types.submodule (
        {...}: {
          options = {
            enabled = mkEnableOption (mdDoc "Bootstrap homeserver configuration");
            repoUrl = mkOption {
              type = types.str;
              default = "";
              description = mdDoc "Git repository URL for the homeserver configuration";
            };
            configPath = mkOption {
              type = types.str;
              default = "${config.neo.volumes.appdata}/configuration";
              description = mdDoc "Path to the homeserver configuration repository";
            };
            template = mkOption {
              type = types.str;
              default = "github:madebydamo/neo#homeserver";
              description = mdDoc "Base template to use for initializing configuration";
            };
            bootstrapEnable = mkEnableOption (mdDoc "Bootstrap the git repository if .git is missing");
            bootstrapMethod = mkOption {
              type = types.enum [
                "template"
                "clone"
              ];
              default = "template";
              description = mdDoc "Bootstrap method: 'template' uses flake init, 'clone' uses git clone from repoUrl";
            };
            bootstrapUser = mkOption {
              type = types.str;
              default = "root";
              description = mdDoc "User to run the bootstrap service as";
            };
            safeDirectoryEnable = mkEnableOption (mdDoc "Configure git safe.directory for configPath");
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
            pluginGeneratorEnable = mkEnableOption (mdDoc "Generate plugins.toml from Nix config");
            pluginGeneratorPlugins = mkOption {
              type = types.attrs;
              default = {};
              description = mdDoc "Attrs converted to plugins.toml";
            };
            rebuildEnable = mkEnableOption (mdDoc "Enable rebuild timer and service");
            rebuildTimer = mkOption {
              type = types.str;
              default = "hourly";
              description = mdDoc "systemd timer OnCalendar value";
            };
            rebuildBranchFormat = mkOption {
              type = types.str;
              default = "%Y%m%d-%H%M%S";
              description = mdDoc "printf format for branch name";
            };
            rebuildBtrfsSubvol = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = mdDoc "Btrfs subvol to snapshot before rebuild";
            };
          };
        }
      );
      default = {};
      description = mdDoc "Homeserver bootstrap configuration";
    };
  };
}
