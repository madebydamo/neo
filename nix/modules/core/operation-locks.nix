# Runtime state for neo operation locks (cli/src/utils/locks/).
#
#   /run/neo/locks/<scope>.lock   flock(2) per scope (system, service_<name>, unit_<unit>)
#   /run/neo/locks/<id>.holder    who holds what (for "Blocked: … in progress" messages)
#   /run/neo/guard/<unit>         unit start guard while a restore has the unit down
#
# neo (web + CLI) runs as homeserver; root (docker-updater) may also flock files here.
#
# Start guard: every service / target asserts that no guard marker exists for it,
# so systemd refuses to start a unit whose data is being restored — whoever asks
# (neo, timers, dependencies, activation, a manual `systemctl start`). Without a
# marker the assertion is a single stat() and always passes. Drop-ins in
# `<type>.d/` apply to every unit of that type.
{...}: {
  flake.modules.nixos.core = {pkgs, ...}: let
    guard = ''
      [Unit]
      AssertPathExists=!/run/neo/guard/%n
    '';
  in {
    systemd.tmpfiles.rules = [
      "d /run/neo 0755 homeserver homeserver -"
      "d /run/neo/locks 0775 homeserver homeserver -"
      "d /run/neo/guard 0755 homeserver homeserver -"
    ];

    systemd.packages = [
      (pkgs.writeTextDir "etc/systemd/system/service.d/50-neo-start-guard.conf" guard)
      (pkgs.writeTextDir "etc/systemd/system/target.d/50-neo-start-guard.conf" guard)
    ];
  };
}
