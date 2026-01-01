{ config, lib, ... }:
{
  imports = [
    ./filebrowser/default.nix
    ./rathole/default.nix
    ./swag/default.nix
  ];
}
