# Hermes skill for tailscale.
{...}: {
  flake.modules.nixos.tailscale-skills = {
    config,
    lib,
    ...
  }: let
    cfg = config.neo.services.tailscale;
    domain = config.neo.services.swag.domain or null;
  in {
    config.neo.services.tailscale.skill.conf = lib.neo.mkServiceSkill {
      service = "tailscale";
      inherit cfg domain;
      description = "Tailscale mesh VPN and tailscaled";
      tags = ["neo" "tailscale" "vpn"];
      body = ''
        ## When to Use
        Mesh access, exit node, advertise routes, SSH over tailnet, split DNS for the SWAG domain over Tailscale.

        ## CLI extras
        ```bash
        tailscale status
        tailscale ip -4
        ```

        ## Credentials
        - `services.tailscale.authKey` (reusable/ephemeral keys from admin console)
        - Treat authKey as secret

        ## Split DNS (`services.tailscale.splitDns`)
        When enabled, host dnsmasq listens only on the Tailscale interface and returns this node's Tailscale IP for the same hostnames Pi-hole would rewrite (subdomains, customDomains, proxyPass, optional apex).
        Pi-hole is not required. If Pi-hole is also enabled, set `services.pihole.localIP` so Pi-hole publishes LAN:53 only; both can run at once.

        Operator (Tailscale admin console → DNS):
        1. Keep MagicDNS if you want machine names. Do not enable Override DNS servers.
        2. Add nameserver → Custom → this node's `tailscale ip -4`.
        3. Restrict to domain: `services.swag.domain` (covers apex and subdomains).
        4. Clients: Tailscale DNS on (`tailscale set --accept-dns=true` on Linux).
        5. ACLs: members need tcp/443 (and tcp/udp 53 on this node).

        ```bash
        tailscale ip -4
        dig @$(tailscale ip -4) <subdomain>.<domain> +short
        systemctl is-active dnsmasq tailscale-split-dns
        ```

        ## Verification
        - `tailscale status` shows connected; ping tailnet IP
        - With split DNS: `dig` against the tailnet IP returns that IP; public DNS is unchanged with Tailscale off
      '';
    };
  };
}
