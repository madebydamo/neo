{
  config,
  lib,
  ...
}: {
  imports = [
    ./backup/default.nix
    ./filebrowser/default.nix
    ./immich/default.nix
    ./openclaw/default.nix
    ./rathole/default.nix
    ./swag/default.nix
  ];
}
