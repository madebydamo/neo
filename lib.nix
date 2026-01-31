{lib}: {
  mkActivationScriptForFile = {
    filePath,
    content,
    mode ? "0644",
    user ? "root",
    group ? "root",
  }:
    with lib; ''
      mkdir -p ${dirOf filePath}
      cat > ${filePath} << 'EOF'
      ${content}
      EOF
      chown ${user}:${group} ${filePath}
      chmod ${mode} ${filePath}
    '';

  mkActivationScriptForDir = {
    dirPath,
    mode ? "0755",
    user ? "root",
    group ? "root",
  }: ''
    mkdir -p ${dirPath}
    chown ${user}:${group} ${dirPath}
    chmod ${mode} ${dirPath}
  '';
}
