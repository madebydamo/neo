{...}: {
  libExtensions.activate-file = {
    neo = {
      mkActivationScriptForFile = config: {
        filePath,
        content,
        mode ? "0644",
        user ? toString config.neo.uid,
        group ? toString config.neo.gid,
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
