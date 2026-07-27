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

        ## Setup UI
        - Unit: `isponsorblocktv-setup` — ttyd on **127.0.0.1:7681** (not LAN); SWAG via `host.docker.internal` + DNAT
        - Always available (`Restart=always`); one browser session at a time (`ttyd --once`), then restarts
        - Setup container is named `isponsorblocktv-setup` and removed on stop (no orphans)

        ## Credentials
        - Pairing codes in UI; no Neo API token

        ## Verification
        - Device paired; segments skipped on TV
        - `systemctl is-active isponsorblocktv-setup` active
        - `docker ps -a` has no leftover `isponsorblocktv-setup` container after a session ends
      '';
    };
  };
}
