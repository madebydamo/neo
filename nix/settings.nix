# User-specific settings for the homeserver (currently empty).
{...}: {
  flake.modules.nixos.settings = {
    config,
    lib,
    pkgs,
    ...
  }: {
    neo.ssh.authorizedKeys = [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC9fFR8aERyyLjqI0aG08BXmSaMemGXK4WK8bLy7Nzmc development"
    ];

    neo.volumes = {
      root = "/var/neo";
      appdata = "/var/neo/DATA/AppData";
      data = "/var/neo/DATA";
      media = "/var/neo/DATA/Media";
      documents = "/var/neo/DATA/Documents";
    };
  };
}
