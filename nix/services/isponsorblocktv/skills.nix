# Hermes skill for isponsorblocktv.
{...}: {
  flake.modules.nixos.isponsorblocktv-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.isponsorblocktv;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.isponsorblocktv.skill.conf = lib.neo.mkServiceSkill {
      service = "isponsorblocktv";
      inherit cfg domain;
      description = "iSponsorBlockTV YouTube ad skip on TVs";
      tags = ["neo" "youtube" "tv"];
      body = ''
        ## When to Use
        TV device pairing, sponsorblock on YouTube app devices.

        ## Setup UI (temporary)
        - Unit: `isponsorblocktv-setup` — ttyd on **127.0.0.1:7681** (not LAN); SWAG via `host.docker.internal` + DNAT
        - Starts on boot for pairing, then auto-stops after **1 hour** (`RuntimeMaxSec`) or after one session (`ttyd --once`)
        - Re-open pairing: `systemctl start isponsorblocktv-setup`
        - Setup container is always named `isponsorblocktv-setup` and removed on stop (no orphans)

        ## Credentials
        - Pairing codes in UI; no Neo API token

        ## Verification
        - Device paired; segments skipped on TV
        - `systemctl is-active isponsorblocktv-setup` inactive after pairing/timeout
        - `docker ps -a` has no leftover `isponsorblocktv-setup` container
      '';
    };
  };
}
