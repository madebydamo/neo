# Hermes NixOS-level dependencies.
# Imports the official hermes-agent NixOS module (native systemd service).
# This is much simpler than OpenClaw's Home Manager setup.
{inputs, ...}: {
  flake.modules.nixos.hermes-dependencies = {
    imports = [
      inputs.hermes-agent.nixosModules.default
    ];
  };
}
