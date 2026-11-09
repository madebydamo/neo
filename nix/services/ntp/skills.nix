# Hermes skill for ntp.
{...}: {
  flake.modules.nixos.ntp-skills = {
    config,
    lib,
    ...
  }: {
    config.neo.services.ntp.skill.conf = lib.neo.mkServiceSkill {
      service = "ntp";
      inherit config;
      description = "LAN chrony NTP service";
      tags = ["neo" "ntp" "time"];
      body = ''
        ## When to Use
        LAN time sync, clock drift, chrony status.

        ## CLI extras
        ```bash
        chronyc tracking
        chronyc sources
        ```

        ## Credentials
        - None

        ## Verification
        - `chronyc tracking` shows synchronized
      '';
    };
  };
}
