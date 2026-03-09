# Homeserver NixOS configuration.
# All service modules are auto-included. Only device-specific modules need listing.
{config, ...}: {
  configurations.nixos.homeserver.modules = [
    config.devices.vm
  ];
}
