{
  config,
  lib,
  ...
}: {
  imports = [
    ./backup/default.nix
    ./filebrowser/default.nix
    ./immich/default.nix
    ./immich-drop/default.nix
    ./openclaw/default.nix
    ./rathole/default.nix
    ./swag/default.nix
    ./tailscale/default.nix
  ];
}
