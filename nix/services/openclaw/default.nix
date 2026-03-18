# OpenClaw NixOS-level dependencies.
# Imports the upstream Home Manager and gateway NixOS modules,
# applies the nix-openclaw overlay, and configures Home Manager defaults.
# The actual service configuration lives in ./configuration/.
{inputs, ...}: {
  flake.modules.nixos.openclaw-dependencies = {
    imports = [
      inputs.nix-openclaw.inputs.home-manager.nixosModules.home-manager
    ];
    nixpkgs.overlays = [inputs.nix-openclaw.overlays.default];
    home-manager.useGlobalPkgs = true;
    home-manager.useUserPackages = true;
    home-manager.overwriteBackup = true;
    home-manager.backupCommand = "rm";
  };
}
