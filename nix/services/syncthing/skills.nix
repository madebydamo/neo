# Hermes skill for syncthing.
{...}: {
  flake.modules.nixos.syncthing-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.syncthing;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.syncthing.skill.conf = lib.neo.mkServiceSkill {
      service = "syncthing";
      inherit cfg domain;
      description = "Syncthing peer sync and GUI";
      tags = ["neo" "syncthing"];
      body = ''
        ## When to Use
        Device pairing, folder sync, GUI issues, ignore patterns.

        ## Architecture notes
        - GUI not published on the host (:8384); only on Docker `internal` via SWAG + tinyauth
        - Sync ports 22000/tcp+udp and 21027/udp stay published for peers
        - insecureAdminAccess patch allows reverse proxy while tinyauth is the edge gate

        ## Credentials
        - GUI auth may be relaxed behind tinyauth — protect the edge
        - Device IDs/API keys in app config/appdata

        ## Procedures
        1. Health-check
        2. Pair devices via GUI
        3. Backup appdata before major resets

        ## Pitfalls
        - Resetting config loses device trust and folder IDs

        ## Verification
        - Devices connected; folders in sync
      '';
    };
  };
}
