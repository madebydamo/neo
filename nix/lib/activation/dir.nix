# Host directory ensure scripts (appdata preStart, volume roots).
{lib, ...}: {
  libExtensions.activate-dir = {
    neo = rec {
      mkActivationScriptForDir = config: {
        dirPath,
        mode ? "0755",
        user ? toString config.neo.core.uid,
        group ? toString config.neo.core.gid,
      }: ''
        if [ ! -e ${dirPath} ]; then
          mkdir -p ${dirPath}
          chown ${user}:${group} ${dirPath}
          chmod ${mode} ${dirPath}
        fi
      '';

      # Ensure one or more directories. Each entry is either a path string or an
      # attrset accepted by mkActivationScriptForDir ({ dirPath, mode?, user?, group? }).
      #
      #   preStart = lib.neo.mkEnsureDirs config [ appdata "${appdata}/data" ];
      #   preStart = lib.neo.mkEnsureDirs config [
      #     { dirPath = "${appdata}/db"; user = "999"; group = "999"; }
      #   ];
      #   system.activationScripts.create-volumes =
      #     lib.neo.mkEnsureDirs config [ config.neo.core.volumes.appdata /* … */ ];
      mkEnsureDirs = config: dirs:
        lib.concatMapStringsSep "\n" (
          dir:
            mkActivationScriptForDir config (
              if builtins.isString dir
              then {dirPath = dir;}
              else dir
            )
        )
        dirs;
    };
  };
}
