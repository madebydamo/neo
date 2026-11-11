# Local/split-horizon DNS name lists and Pi-hole publish-port helpers.
# Used by Pi-hole (LAN IP), Tailscale split DNS (tailnet IP) and NetBird's
# DNS zones (set in its control plane; Neo only knows they were declared).
{lib, ...}: {
  libExtensions.dns = {
    neo = rec {
      # FQDNs rewritten to a single A/AAAA target (LAN or Tailscale IP).
      localDnsNames = {
        domain,
        onlySubdomains ? true,
        subdomains ? [],
        customDomains ? [],
        proxyPassDomains ? [],
      }: let
        hasDomain = domain != null && domain != "";
        fqdns = lib.optionals hasDomain (map (sub: "${sub}.${domain}") subdomains);
        apex = lib.optional (hasDomain && !onlySubdomains) domain;
      in
        lib.unique (fqdns ++ customDomains ++ proxyPassDomains ++ apex);

      localDnsNamesFromConfig = config: let
        swagCfg = config.neo.services.swag or {};
        appServices = lib.filterAttrs (
          _: v: (v.enabled or false) && (v.subdomain or null) != null
        ) (config.neo.services or {});
      in
        localDnsNames {
          domain = swagCfg.domain or null;
          onlySubdomains = swagCfg.onlySubdomains or true;
          subdomains = lib.catAttrs "subdomain" (lib.attrValues appServices);
          customDomains = lib.concatLists (lib.catAttrs "customDomains" (lib.attrValues appServices));
          proxyPassDomains = lib.attrNames (swagCfg.proxyPass or {});
        };

      # True only when the Tailscale split-DNS dnsmasq unit is actually enabled.
      splitDnsActive = config:
        (config.neo.services.tailscale.enabled or false)
        && (config.neo.services.tailscale.splitDns or false);

      # True when the NetBird client says its network answers this server's
      # app names (a DNS zone in the control plane, checked by netbird-check).
      netbirdAppNamesActive = config:
        (config.neo.services.netbird.enabled or false)
        && (config.neo.services.netbird.appNames or false);

      # True when the NetBird client runs its resolver and owns host DNS.
      netbirdDnsActive = config:
        (config.neo.services.netbird.enabled or false)
        && (config.neo.services.netbird.acceptDns or false);

      # Address ranges of the NetBird network for SWAG's ingress class.
      netbirdRanges = config:
        lib.optional (config.neo.services.netbird.enabled or false)
        config.neo.services.netbird.networkRange;

      # True when service hostnames resolve to a private address (LAN via
      # Pi-hole rewrite, tailnet via split DNS, or NetBird's zones). Pi-hole only
      # counts when localIP is set so those names actually rewrite off the public
      # A record.
      privateHostnameDnsActive = config: let
        pihole = config.neo.services.pihole or {};
        ip = pihole.localIP or null;
        piholeRewrites = (pihole.enabled or false) && ip != null && ip != "";
      in
        piholeRewrites || splitDnsActive config || netbirdAppNamesActive config;

      # Docker -p specs for Pi-hole's host DNS port.
      piholeDnsPublishPorts = {
        splitDnsActive,
        localIP,
      }:
        if splitDnsActive
        then [
          "${localIP}:53:53/tcp"
          "${localIP}:53:53/udp"
        ]
        else [
          "53:53/tcp"
          "53:53/udp"
        ];
    };
  };
}
