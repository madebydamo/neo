{...}: {
  libExtensions.activate-file = {
    neo = {
      mkActivationScriptForFile = config: {
        filePath,
        content,
        mode ? "0644",
        user ? toString config.neo.core.uid,
        group ? toString config.neo.core.gid,
      }: ''
        mkdir -p ${dirOf filePath}
        cat > ${filePath} << 'ACTEOF'
        ${content}
        ACTEOF
        chown ${user}:${group} ${filePath}
        chmod ${mode} ${filePath}
      '';
    };
  };
}
