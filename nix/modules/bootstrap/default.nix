# Bootstrap units live under services/system-updater (enabled with system-updater).
# This module only registers the neo-cli option (option.nix) and CLI package (cli.nix).
{...}: {
  flake.modules.nixos.bootstrap = {};
}
