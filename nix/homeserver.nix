# Declares the homeserver NixOS configuration, pulling in all NixOS modules.
{
  config,
  inputs,
  ...
}: let
  inherit (config.flake.modules) nixos;
in {
  configurations.nixos.homeserver.modules = [
    inputs.nix-openclaw.nixosModules.openclaw-gateway
    {nixpkgs.overlays = [inputs.nix-openclaw.overlays.default];}
    nixos.options
    nixos.core
    nixos.settings
    nixos.device-vm
    nixos.backup-option
    nixos.backup
    nixos.filebrowser-option
    nixos.filebrowser
    nixos.filebrowser-swag
    nixos.immich-option
    nixos.immich
    nixos.immich-swag
    nixos.immich-drop-option
    nixos.immich-drop
    nixos.immich-drop-swag
    nixos.openclaw-option
    nixos.openclaw
    nixos.openclaw-swag
    nixos.rathole-option
    nixos.rathole
    nixos.swag-option
    nixos.swag
    nixos.tailscale-option
    nixos.tailscale
  ];
}
