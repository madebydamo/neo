# Updates that only take effect after a restart (neo.core.rebootEpoch).
#
# Some changes never reach a running system through a switch: NixOS does not
# restart Docker (restartIfChanged = false), the kernel and its modules wait for
# a boot, and so on. Such a change raises `rebootEpoch` (below, with its reason).
# Every system records the number in $out/neo-reboot-epoch; a restart is due
# while the system switched to has a higher number than the one the machine
# booted (/run/booted-system).
#
#   neo-reboot-required        exit 0 and print why when a restart is due
#
# `neo activate` marks such an activation reboot_pending; the system updater
# restarts the server after it (system-updater.rebootWhenRequired).
#
# Raise the number (never lower it) and add a line:
#   1  Docker address pools of 172.16.0.0/12 (docker daemon settings)
{...}: {
  flake.modules.nixos.core-reboot = {
    config,
    lib,
    pkgs,
    ...
  }: let
    epoch = config.neo.core.rebootEpoch;
  in {
    options.neo.core.rebootEpoch = lib.mkOption {
      type = lib.types.ints.unsigned;
      default = 1;
      internal = true;
      visible = false;
      description = "Raised by Neo for changes that need a restart; see nix/modules/core/reboot.nix";
    };

    config = {
      system.systemBuilderCommands = ''
        echo ${toString epoch} > $out/neo-reboot-epoch
      '';

      environment.systemPackages = [
        (pkgs.writeShellScriptBin "neo-reboot-required" ''
          # Exit 0 (and say why) when the system switched to needs a restart.
          read_epoch() { cat "$1/neo-reboot-epoch" 2>/dev/null || echo 0; }
          booted=$(read_epoch /run/booted-system)
          target=$(read_epoch "''${1:-/nix/var/nix/profiles/system}")
          if [ "$target" -gt "$booted" ]; then
            echo "This update needs a restart of the server to take full effect (restart level $booted → $target)."
            exit 0
          fi
          exit 1
        '')
      ];
    };
  };
}
