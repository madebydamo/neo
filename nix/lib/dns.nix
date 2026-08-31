# Local/split-horizon DNS name lists and Pi-hole publish-port helpers.
# Used by Pi-hole (LAN IP) and Tailscale split DNS (tailnet IP).
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

      # True when service hostnames resolve to a private address (LAN via
      # Pi-hole rewrite, or tailnet via split DNS). Pi-hole only counts when
      # localIP is set so those names actually rewrite off the public A record.
      privateHostnameDnsActive = config: let
        pihole = config.neo.services.pihole or {};
        ip = pihole.localIP or null;
        piholeRewrites = (pihole.enabled or false) && ip != null && ip != "";
      in
        piholeRewrites || splitDnsActive config;

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
