{...}: {
  libExtensions.activate-dir = {
    neo = {
      mkActivationScriptForDir = config: {
        dirPath,
        mode ? "0755",
        user ? toString config.neo.uid,
        group ? toString config.neo.gid,
      }: ''
        mkdir -p ${dirPath}
        chown -R ${user}:${group} ${dirPath}
        chmod -R ${mode} ${dirPath}
      '';
    };
  };
}
