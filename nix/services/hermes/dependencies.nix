# Hermes NixOS-level dependencies.
# Imports the official hermes-agent NixOS module (native systemd service).
{inputs, ...}: {
  flake.modules.nixos.hermes-dependencies = {
    imports = [
      inputs.hermes-agent.nixosModules.default
    ];
  };
}
