# User-specific settings for the homeserver (currently empty).
{...}: {
  flake.modules.nixos.settings = {
    config,
    lib,
    pkgs,
    ...
  }: {
    neo.ssh.authorizedKeys = lib.mkDefault [
      "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIC9fFR8aERyyLjqI0aG08BXmSaMemGXK4WK8bLy7Nzmc development"
    ];

    neo.volumes = {
      root = lib.mkDefault "/var/neo";
      appdata = lib.mkDefault "/var/neo/DATA/AppData";
      data = lib.mkDefault "/var/neo/DATA";
      media = lib.mkDefault "/var/neo/DATA/Media";
      documents = lib.mkDefault "/var/neo/DATA/Documents";
    };
  };
}
