# Hermes skill for vpn.
{...}: {
  flake.modules.nixos.vpn-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.vpn;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.vpn.skill.conf = lib.neo.mkServiceSkill {
      service = "vpn";
      inherit cfg domain;
      description = "Gluetun shared WireGuard outbound VPN";
      tags = ["neo" "vpn" "gluetun"];
      body = ''
        ## When to Use
        Outbound WireGuard for services that set vpn options; killswitch issues.

        ## Architecture notes
        - gluetun container provides network for opted-in services
        - Options: wireguard private/preshared keys, addresses, provider/countries

        ## Credentials
        - WireGuard keys in `services.vpn` — highly sensitive

        ## Pitfalls
        - Misconfig breaks all VPN-attached services' networking

        ## Verification
        - Gluetun healthy; egress IP matches provider
      '';
    };
  };
}
