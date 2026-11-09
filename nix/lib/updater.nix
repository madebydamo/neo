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

      # Start of a run script: sets run_id, started_at, hist and log_file, and
      # sends all further output to the run's log too.
      updaterRunPrelude = dir: ''
        run_id=$(date -u +%Y%m%dT%H%M%SZ)-$$
        started_at=$(date -Iseconds)
        mkdir -p ${dir}
        hist=${dir}/$run_id.json
        log_file=${dir}/$run_id.log
        exec > >(tee -a "$log_file") 2>&1'';
    };
  };
}
