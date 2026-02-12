{lib}: {
  mkActivationScriptForFile = config: {
    filePath,
    content,
    mode ? "0666",
    user ? toString config.neo.uid,
    group ? toString config.neo.gid,
  }:
    with lib; ''
      mkdir -p ${dirOf filePath}
      cat > ${filePath} << 'EOF'
      ${content}
      EOF
      chown ${user}:${group} ${filePath}
      chmod ${mode} ${filePath}
    '';

  mkActivationScriptForDir = config: {
    dirPath,
    mode ? "0777",
    user ? toString config.neo.uid,
    group ? toString config.neo.gid,
  }: ''
    mkdir -p ${dirPath}
    chown ${user}:${group} ${dirPath}
    chmod ${mode} ${dirPath}
  '';
}
