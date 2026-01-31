{
  config,
  lib,
  ...
}: {
  imports = [
    ./filebrowser/default.nix
    ./openclaw/default.nix
    ./rathole/default.nix
    ./swag/default.nix
  ];
}
