# Vikunja implementation.
# Uses the nixpkgs package via services.vikunja (nixos-26.05, Neo's pin).
# Listens on the host so SWAG can proxy to the docker bridge gateway.
{...}: {
  flake.modules.nixos.vikunja = {
    config,
    lib,
    ...
  }:
    with lib; let
      cfg = config.neo.services.vikunja;
      domain = config.neo.services.swag.domain or "localhost";
      publicHost = "${cfg.subdomain}.${domain}";
    in {
      config = mkIf cfg.enabled {
        services.vikunja = {
          enable = true;
          frontendScheme = "https";
          frontendHostname = publicHost;
          # Reachable from the SWAG container via the internal-network gateway.
          address = "0.0.0.0";
          port = cfg.port;
          settings = {
            service = {
              enableregistration = cfg.allowRegistration;
              enableemailreminders = false;
              # Public URL must match the SWAG vhost, with a trailing slash.
              publicurl = "https://${publicHost}/";
            };
            database = {
              type = "sqlite";
              path = "/var/lib/vikunja/vikunja.db";
            };
          };
        };
      };
    };
}
