{...}: {
  libExtensions.activate-dir = {
    neo = {
      mkActivationScriptForDir = config: {
        dirPath,
        mode ? "0755",
        user ? toString config.neo.uid,
        group ? toString config.neo.gid,
      }: ''
        if [ ! -e ${dirPath} ]; then
          mkdir -p ${dirPath}
          chown ${user}:${group} ${dirPath}
          chmod ${mode} ${dirPath}
        fi
      '';
    };
  };
}
