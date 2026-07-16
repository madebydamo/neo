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

        ## Credentials
        - Pairing codes in UI; no Neo API token

        ## Verification
        - Device paired; segments skipped on TV
      '';
    };
  };
}
