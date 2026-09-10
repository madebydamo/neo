# Per-service ingress posture (LAN / Tailscale / public web via rathole).
#
# Rathole is a shared tunnel (HTTP 80 + SWAG PROXY-protocol HTTPS). There are
# no per-app rathole listen ports. "web" off means this vhost is denied on the
# PROXY-protocol listener (and on direct public 443). HTTP-01 on port 80 is
# never gated here.
{lib, ...}: let
  inherit (lib) concatMapStrings elem optionalString unique;

  defaultIngress = ["local" "tailscale" "web"];

  hostKeys = svc:
    unique (
      ["~*^${svc.subdomain}(\\.|$)"]
      ++ (svc.customDomains or [])
    );

  allows = svc: variant:
    elem variant (svc.ingress or defaultIngress);

  denyLines = services: variant:
    concatMapStrings (
      svc:
        optionalString (!allows svc variant) (
          concatMapStrings (h: "    ${h} 0;\n") (hostKeys svc)
        )
    ) (lib.attrValues services);
in {
  libExtensions.ingress = {
    neo = rec {
      ingressVariants = defaultIngress;

      # HTTP-context maps + geo. Included from nginx.conf (see swag-patcher).
      mkIngressMapsConf = {
        services,
        proxyProtocolPort ? 8443,
      }: ''
        ## Neo-managed — per-service ingress classification and allow maps.
        ## Class: PROXY-protocol port → web; RFC1918/loopback → local;
        ## Tailscale CGNAT/ULA → tailscale; other 443 → web (direct public).

        geo $ingress_is_lan {
            default 0;
            10.0.0.0/8 1;
            172.16.0.0/12 1;
            192.168.0.0/16 1;
            127.0.0.0/8 1;
            ::1 1;
            fc00::/7 1;
            fe80::/10 1;
        }

        geo $ingress_is_tailscale {
            default 0;
            100.64.0.0/10 1;
            fd7a:115c:a1e0::/48 1;
        }

        map $server_port $ingress_via_web {
            default 0;
            ${toString proxyProtocolPort} 1;
        }

        map "$ingress_via_web:$ingress_is_lan:$ingress_is_tailscale" $ingress_class {
            default web;
            "1:0:0" web;
            "1:1:0" web;
            "1:0:1" web;
            "1:1:1" web;
            "0:1:0" local;
            "0:0:1" tailscale;
            "0:1:1" tailscale;
            "0:0:0" web;
        }

        map $host $ingress_allow_local {
            default 1;
        ${denyLines services "local"}}

        map $host $ingress_allow_tailscale {
            default 1;
        ${denyLines services "tailscale"}}

        map $host $ingress_allow_web {
            default 1;
        ${denyLines services "web"}}
      '';

      # Server-context deny. Included from listen-https.conf so every vhost
      # (mkSubdomainProxyConf and hand-written) is gated.
      ingressAccessConf = ''
        ## Neo-managed — deny requests whose ingress class is not selected.
        set $ingress_ok 1;
        if ($ingress_class = local) {
            set $ingress_ok $ingress_allow_local;
        }
        if ($ingress_class = tailscale) {
            set $ingress_ok $ingress_allow_tailscale;
        }
        if ($ingress_class = web) {
            set $ingress_ok $ingress_allow_web;
        }
        if ($ingress_ok = 0) {
            return 404;
        }
      '';
    };
  };
}
