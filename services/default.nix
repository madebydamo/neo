{ config, lib, ... }:
{
  imports = [
    ./filebrowser/default.nix
    ./swag/default.nix
  ];
}
