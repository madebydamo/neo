# Host directories a container needs (its appdata and subfolders), created when
# missing at the start of the container's unit, before anything else the
# service puts in that preStart (only the network check of docker-network.nix
# runs earlier). Owner neo uid:gid and mode 0755 unless an entry says otherwise
# (lib.neo.mkEnsureDirs). Lists for one container from several modules add up.
#
#   neo.core.containerDirs.<container> = [
#     appdata
#     "${appdata}/data"
#     { dirPath = "${appdata}/db"; user = "999"; group = "999"; }
#   ];
{...}: {
  flake.modules.nixos.core-container-dirs = {
    config,
    lib,
    ...
  }: {
    options.neo.core.containerDirs = lib.mkOption {
      type = with lib.types; attrsOf (listOf (either str (attrsOf str)));
      default = {};
      internal = true;
      description = "Host directories created before each container starts (docker-<container> preStart).";
    };

    config.systemd.services = lib.mapAttrs' (container: dirs:
      lib.nameValuePair "docker-${container}" {
        preStart = lib.mkOrder 750 (lib.neo.mkEnsureDirs config dirs);
      })
    (lib.filterAttrs (_: dirs: dirs != []) config.neo.core.containerDirs);
  };
}
