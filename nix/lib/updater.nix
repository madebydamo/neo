# Shared paths for system/docker updater run history.
# Layout (each updater's mkAppdata points at its subdirectory so the web UI
# can Clear appdata independently):
#
#   <appdata>/updater/docker/          docker-updater.appdata
#     <utc>-<pid>.json                 one file per run (never overwritten)
#     <utc>-<pid>.log
#     last.json                        symlink to the *current* run's json
#   <appdata>/updater/system/          system-updater.appdata
#     …
#
# last.json is retargeted at the start of a run (in-progress stub) so a crash
# cannot leave it pointing at a previous successful run.
{lib, ...}: let
  inherit ((import ./activation/dir.nix {inherit lib;}).libExtensions.activate-dir.neo) mkEnsureDirs;
in {
  libExtensions.updater = {
    neo = {
      mkUpdaterPaths = appdata: rec {
        stateDir = "${appdata}/updater";
        dockerHistoryDir = "${stateDir}/docker";
        systemHistoryDir = "${stateDir}/system";
        dockerLast = "${dockerHistoryDir}/last.json";
        systemLast = "${systemHistoryDir}/last.json";
      };

      # Activation script for history directories the web UI (homeserver) reads
      # and root or homeserver writes.
      mkUpdaterHistoryDirs = config: dirs:
        mkEnsureDirs config (map (dirPath: {
            inherit dirPath;
            mode = "0775";
            user = "homeserver";
            group = "homeserver";
          })
          dirs);
    };
  };
}
