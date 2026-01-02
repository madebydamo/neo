{ lib }:

{
  mkActivationScriptForFile = {
    filePath,
    content,
    mode ? "0644",
    user ? "root",
    group ? "root"
  }: with lib; ''
    mkdir -p ${dirOf filePath}
    cat > ${filePath} << 'EOF'
    ${content}
    EOF
    chown ${user}:${group} ${filePath}
    chmod ${mode} ${filePath}
  '';
}